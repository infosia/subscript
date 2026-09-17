//! The budgeted compile child (`specs/blocks/compiler.md` §109.2 rule 6).
//!
//! The parser is external, and its work and memory on a hostile shape
//! are not bounded by construction. §109.0 guarantee 4 therefore holds
//! by a process boundary: under the sandbox profile the CLI re-execs
//! itself, the child compiles with a memory budget and a time budget,
//! and a child that passes either budget is one S026 at the entry file.
//! Under the default profile nothing spawns.
//!
//! The child sets its own memory budget where the host holds one. macOS
//! refuses `RLIMIT_AS`, so the parent holds the budget there: it reads
//! the child's resident bytes at every poll.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use subscript_compiler::{render_diagnostics, Diagnostic, Pos, Profile, RuleCode};

use crate::{Failure, PROGRAM_ERROR, SUCCESS, USAGE_ERROR};

/// The private flag the parent adds to the child's arguments.
///
/// [`crate::execute`] removes it before any subcommand reads the
/// arguments, so no argument parser sees it and no usage text names it.
pub(crate) const CHILD_FLAG: &str = "--compile-child";

/// The environment variable the parent sets on the child it spawns
/// (§109.2 rule 6).
///
/// The value is the parent's own process id. The child runs only when
/// the parent marks it this way, so [`CHILD_FLAG`] from a command line
/// is a usage error on every subcommand.
pub(crate) const CHILD_MARKER_VARIABLE: &str = "SUBSCRIPT_COMPILE_CHILD";

/// The usage error for [`CHILD_FLAG`] with no [`CHILD_MARKER_VARIABLE`].
const UNMARKED_CHILD_MESSAGE: &str = "the compile child flag is not a user option";

/// The environment variable that replaces the compile child's time
/// budget, in seconds (§109.2 rule 6).
///
/// A test sets it to reach the time-budget stop in seconds instead of
/// [`TIME_BUDGET_SECONDS`]. A value that is not a count of seconds, and
/// a value over [`TIME_BUDGET_CEILING_SECONDS`], leave the contract's
/// budget in place.
pub const TIME_BUDGET_VARIABLE: &str = "SUBSCRIPT_COMPILE_TIME_BUDGET_SECONDS";

/// The test-only variable that ends the compile child at its first line
/// (§109.2 rule 6).
///
/// The parent's reading of an abnormal end has no other deterministic
/// source: the exit code and the signal number must come from the host,
/// and not from a number this crate also writes. `panic` ends the child
/// through the panic runtime, whose exit code the runtime picks.
/// `sigterm` sends the child the signal `kill -TERM` sends. Every other
/// value, and no value, leaves the child alone.
pub const CHILD_STOP_VARIABLE: &str = "SUBSCRIPT_COMPILE_CHILD_STOP";

/// The time budget the parent gives the child, in seconds
/// (§109.2 rule 6).
pub(crate) const TIME_BUDGET_SECONDS: u64 = 300;

/// The largest time budget [`TIME_BUDGET_VARIABLE`] can ask for, in
/// seconds.
///
/// A day is longer than every compile this project measured, and the
/// ceiling holds the deadline arithmetic inside `Instant`'s range.
const TIME_BUDGET_CEILING_SECONDS: u64 = 86_400;

/// The address space the child may hold, in bytes (§109.2 rule 6).
///
/// The address space must hold the compile thread's stack reservation
/// and the compiler's heap. §109.2a sizes that stack from the deepest
/// nesting a file of S026's byte limit can spell: 8,589,934,592 bytes
/// unoptimized and 2,147,483,648 optimized. Each budget is that build's
/// stack plus the heap a compile may take, 4 GiB unoptimized and 2 GiB
/// optimized. A budget at or under the stack refuses the compile thread
/// itself, and the profile then checks nothing (§109.2a).
pub(crate) const MEMORY_BUDGET_BYTES: u64 = if cfg!(debug_assertions) {
    12_884_901_888
} else {
    4_294_967_296
};

// The stack is the compiler crate's number and the budget is this
// module's, so the two are comparable: a budget that cannot hold the
// reservation turns every profile compile into the §109.2a refusal.
const _: () = assert!(
    MEMORY_BUDGET_BYTES > subscript_compiler::COMPILE_THREAD_STACK_BYTES as u64,
    "the memory budget must hold the compile thread's stack reservation"
);

/// The S026 message for a child that passed its memory budget.
const MEMORY_MESSAGE: &str = "the compiler passed its memory budget";

/// The S026 message for a child that passed its time budget.
const TIME_MESSAGE: &str = "the compiler passed its time budget";

/// The text the Rust runtime writes before it ends a process that
/// cannot allocate: `memory allocation of <n> bytes failed`.
///
/// Linux turns the budget into a failed allocation through
/// `RLIMIT_AS`, and Windows through the Job Object's process memory
/// limit. The runtime then ends the child: on `SIGABRT` under a signal
/// host, and with `__fastfail`'s own exit code on Windows. This text is
/// what separates that end from every other abnormal end
/// (§109.2 rule 6).
#[cfg(any(unix, windows))]
const ALLOCATION_FAILURE_TEXT: &[u8] = b"memory allocation of";

/// The resident bytes a child must have held for the parent to read a
/// `SIGKILL` as the system's own memory kill (§109.2 rule 6).
///
/// macOS reclaims a child's pages before it kills the child, so the
/// reading at the kill is a small part of the largest reading: measured
/// on the same-label chain, the largest reading is 9.75 to 9.98 GB
/// against a reading of 2.1 to 2.3 GB at the kill. The parent therefore
/// compares the largest reading it took, and one half of the budget is
/// what separates a child that was holding memory from one that was
/// not.
#[cfg(unix)]
fn system_memory_kill_floor() -> u64 {
    MEMORY_BUDGET_BYTES / 2
}

/// How often the parent asks whether the child has ended.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Whether this process holds the budget or runs inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// The process the caller started. It re-execs itself under the
    /// sandbox profile.
    Parent,
    /// The budgeted child. It compiles and spawns nothing.
    Child,
}

/// What [`apply_memory_budget`] did on this host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoryBudget {
    /// The host holds this process to [`MEMORY_BUDGET_BYTES`].
    Set,
    /// The host refused the limit. macOS refuses `RLIMIT_AS`, and the
    /// parent holds the budget there: it reads the child's resident
    /// bytes at every poll and kills a child over the budget. The
    /// parent's time budget and its reading of an abnormal exit hold
    /// guarantee 4 on every other such host, because a child the system
    /// kills is one S026.
    Refused,
}

/// Removes [`CHILD_FLAG`] from `args` and answers this process's role.
///
/// The flag is private. Only the parent sets
/// [`CHILD_MARKER_VARIABLE`], so the flag with no marker is a usage
/// error, whichever subcommand carries it (§109.2 rule 6).
pub(crate) fn take_role(args: &mut Vec<OsString>) -> Result<Role, Failure> {
    take_role_of(args, marked())
}

/// Answers this process's role for a process that `marked` describes.
fn take_role_of(args: &mut Vec<OsString>, marked: bool) -> Result<Role, Failure> {
    let before = args.len();
    args.retain(|arg| arg != CHILD_FLAG);
    if args.len() == before {
        return Ok(Role::Parent);
    }
    if marked {
        Ok(Role::Child)
    } else {
        Err(Failure::usage(UNMARKED_CHILD_MESSAGE))
    }
}

/// True when a parent marked this process as its compile child.
fn marked() -> bool {
    std::env::var_os(CHILD_MARKER_VARIABLE).is_some_and(|value| !value.is_empty())
}

/// Ends this child as [`CHILD_STOP_VARIABLE`] asks (§109.2 rule 6).
///
/// The child calls this at its first line, before it reads a source, so
/// the end the parent classifies is the only work of the run.
pub(crate) fn apply_test_only_stop() {
    match std::env::var(CHILD_STOP_VARIABLE).ok().as_deref() {
        Some("panic") => panic!("{CHILD_STOP_VARIABLE}=panic ends this compile child"),
        Some("sigterm") => raise_termination(),
        _ => {}
    }
}

/// Sends this process the signal `kill -TERM` sends.
#[cfg(unix)]
fn raise_termination() {
    // SAFETY: `raise` sends one signal to the calling process.
    unsafe { libc::raise(libc::SIGTERM) };
}

/// Sends this process the signal `kill -TERM` sends.
///
/// A host with no POSIX signals has none to send.
#[cfg(not(unix))]
fn raise_termination() {}

/// True when this process must run `profile`'s compile in a child
/// (§109.2 rule 6).
///
/// The default profile spawns nothing, and the child never spawns a
/// child of its own.
pub(crate) fn spawns(role: Role, profile: Profile) -> bool {
    role == Role::Parent && profile == Profile::Sandbox
}

/// Sets this process's memory budget (§109.2 rule 6).
///
/// Unix sets `RLIMIT_AS` from the child's first line, before the
/// compile. A host that refuses the limit leaves the bound to the
/// parent.
#[cfg(unix)]
pub(crate) fn apply_memory_budget() -> MemoryBudget {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `getrlimit` writes one `rlimit` through the pointer.
    if unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut current) } != 0 {
        return MemoryBudget::Refused;
    }
    let wanted = libc::rlimit {
        rlim_cur: budgeted_soft_limit(current.rlim_max),
        rlim_max: current.rlim_max,
    };
    // SAFETY: `setrlimit` reads one `rlimit` through the pointer.
    if unsafe { libc::setrlimit(libc::RLIMIT_AS, &wanted) } != 0 {
        return MemoryBudget::Refused;
    }
    MemoryBudget::Set
}

/// The soft `RLIMIT_AS` the budget asks for under a `hard` limit.
///
/// A host whose hard limit is already under the budget keeps it: a soft
/// limit over the hard one is refused.
#[cfg(unix)]
fn budgeted_soft_limit(hard: libc::rlim_t) -> libc::rlim_t {
    if hard == libc::RLIM_INFINITY {
        MEMORY_BUDGET_BYTES
    } else {
        MEMORY_BUDGET_BYTES.min(hard)
    }
}

/// Sets this process's memory budget (§109.2 rule 6).
///
/// The process creates the Job Object and assigns itself from its first
/// line, so no allocation of the child runs outside the limit. The job
/// handle stays open for the life of the process: the limit holds while
/// the job holds the process.
///
/// The job also carries `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Every
/// process the child starts joins the job, and the child holds the only
/// handle to it. A kill of the child therefore closes that handle, and
/// the job ends the C compiler with it.
#[cfg(windows)]
pub(crate) fn apply_memory_budget() -> MemoryBudget {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let Ok(limit) = usize::try_from(MEMORY_BUDGET_BYTES) else {
        return MemoryBudget::Refused;
    };
    // SAFETY: an unnamed job with default security attributes.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return MemoryBudget::Refused;
    }
    let information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
            LimitFlags: JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            ..Default::default()
        },
        ProcessMemoryLimit: limit,
        ..Default::default()
    };
    let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .unwrap_or(u32::MAX);
    // SAFETY: `information` is one live value of the class named, and
    // `size` is its own size.
    let set = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&information).cast(),
            size,
        )
    };
    if set == 0 {
        return MemoryBudget::Refused;
    }
    // SAFETY: `job` is the handle above, and the second handle is this
    // process's own pseudo-handle.
    let assigned = unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    if assigned == 0 {
        return MemoryBudget::Refused;
    }
    MemoryBudget::Set
}

/// Sets this process's memory budget (§109.2 rule 6).
///
/// A host that is neither unix nor Windows has no limit here, and the
/// parent's time budget is the whole bound.
#[cfg(not(any(unix, windows)))]
pub(crate) fn apply_memory_budget() -> MemoryBudget {
    MemoryBudget::Refused
}

/// Runs one profile compile in a budgeted child and answers its exit
/// code (§109.2 rule 6).
///
/// `command` is the subcommand, `args` the arguments the parent read,
/// and `entry` the source a budget stop names. The child's stdout and
/// stderr pass through unchanged. A child that passed a budget, and a
/// child that stopped abnormally, is one S026 at `entry`.
///
/// `run` and `build` do their whole work in the child, so the budgets
/// cover the program run and the C compile as well as the compile. The
/// run's own bounds are the Context's (§109.4).
pub(crate) fn compile_in_child<O: Write, E: Write>(
    command: &str,
    args: &[OsString],
    entry: &Path,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<u8, Failure> {
    let program = std::env::current_exe()
        .map_err(|error| Failure::usage(format!("locate this executable: {error}")))?;
    let mut spawning = Command::new(program);
    spawning
        .arg(command)
        .args(args)
        .arg(CHILD_FLAG)
        .env(CHILD_MARKER_VARIABLE, std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_process_group(&mut spawning);
    let child = spawning
        .spawn()
        .map_err(|error| Failure::usage(format!("start the compile child: {error}")))?;
    // Every path out of this function drops the guard, the parent's own
    // unwind included, and the drop kills the child's whole process
    // group (§109.2 rule 6).
    let mut guard = ChildGuard::new(child);
    let child_stdout = guard
        .child
        .stdout
        .take()
        .ok_or_else(|| Failure::usage("the compile child has no stdout pipe"))?;
    let child_stderr = guard
        .child
        .stderr
        .take()
        .ok_or_else(|| Failure::usage("the compile child has no stderr pipe"))?;

    // The readers drain both pipes for the whole wait, so a child that
    // writes more than one pipe buffer never blocks on the parent.
    let (ended, out_bytes, err_bytes) = std::thread::scope(|scope| {
        let reading_out = scope.spawn(move || read_all(child_stdout));
        let reading_err = scope.spawn(move || read_all(child_stderr));
        let ended = wait_within(&mut guard.child, time_budget());
        (ended, reading_out.join(), reading_err.join())
    });
    let (stop, largest_resident) = ended?;
    guard.reaped();
    let out_bytes =
        out_bytes.map_err(|_| Failure::usage("reading the compile child's stdout failed"))?;
    let err_bytes =
        err_bytes.map_err(|_| Failure::usage("reading the compile child's stderr failed"))?;

    // §109.2 rule 6: the child's bytes reach the caller first, and the
    // classification of the child follows them. A failed write of this
    // process's own stdout is reported on stderr and never replaces the
    // child's outcome.
    let write_error = stdout
        .write_all(&out_bytes)
        .and_then(|()| stdout.flush())
        .err();
    let _ = stderr.write_all(&err_bytes).and_then(|()| stderr.flush());
    if let Some(error) = write_error {
        let _ = writeln!(stderr, "subscript: write program stdout: {error}");
        let _ = stderr.flush();
    }

    match stop {
        Stop::Killed(passed) => Err(budget_stop(entry, passed.message())),
        Stop::Exited(status) => classify(status, &err_bytes, largest_resident)
            .map_err(|message| budget_stop(entry, &message)),
    }
}

/// The exit code of a child that ended on its own, or the S026 message
/// for a child that stopped abnormally (§109.2 rule 6).
///
/// Exit codes 0, 1, and 2 pass through. Every other code, and every
/// signal but the system's own memory kill, is an abnormal stop.
fn classify(
    status: ExitStatus,
    child_stderr: &[u8],
    largest_resident: Option<u64>,
) -> Result<u8, String> {
    match status.code() {
        Some(0) => Ok(SUCCESS),
        Some(1) => Ok(PROGRAM_ERROR),
        Some(2) => Ok(USAGE_ERROR),
        Some(code) => Err(exit_code_message(code, child_stderr)),
        None => Err(signal_message(status, child_stderr, largest_resident)),
    }
}

/// The S026 message for a child that ended with a code no outcome
/// describes (§109.2 rule 6).
///
/// The Job Object's process memory limit fails an allocation, the Rust
/// runtime writes [`ALLOCATION_FAILURE_TEXT`], and `__fastfail` then
/// ends the child with a code of its own. That text is what separates
/// the budget from every other abnormal end, as it does under
/// `RLIMIT_AS`. Windows reports no signal, so the code alone cannot
/// carry the difference.
#[cfg(windows)]
fn exit_code_message(code: i32, child_stderr: &[u8]) -> String {
    if holds(child_stderr, ALLOCATION_FAILURE_TEXT) {
        return String::from(MEMORY_MESSAGE);
    }
    format!("the compiler stopped abnormally (exit code {code})")
}

/// The S026 message for a child that ended with a code no outcome
/// describes (§109.2 rule 6).
///
/// A host with signals reports the budget's failed allocation as
/// `SIGABRT`, which [`signal_message`] reads. An exit code there is
/// the child's own, so it stands alone.
#[cfg(not(windows))]
fn exit_code_message(code: i32, _child_stderr: &[u8]) -> String {
    format!("the compiler stopped abnormally (exit code {code})")
}

/// The S026 message for a child that no exit code describes
/// (§109.2 rule 6).
#[cfg(unix)]
fn signal_message(
    status: ExitStatus,
    child_stderr: &[u8],
    largest_resident: Option<u64>,
) -> String {
    use std::os::unix::process::ExitStatusExt;
    let Some(signal) = status.signal() else {
        return String::from("the compiler stopped abnormally (no exit code)");
    };
    if system_memory_kill(signal, child_stderr, largest_resident) {
        return String::from(MEMORY_MESSAGE);
    }
    format!("the compiler stopped abnormally (signal {signal})")
}

/// The S026 message for a child that no exit code describes
/// (§109.2 rule 6).
///
/// A host with no POSIX signals reports a code for every end, so this
/// message stands for the end the host does not describe.
#[cfg(not(unix))]
fn signal_message(
    _status: ExitStatus,
    _child_stderr: &[u8],
    _largest_resident: Option<u64>,
) -> String {
    String::from("the compiler stopped abnormally (no exit code)")
}

/// True when the signal that ended the child is the system's own memory
/// kill (§109.2 rule 6).
///
/// macOS refuses `RLIMIT_AS`, so a child over the machine's own limit
/// ends on `SIGKILL`; the largest reading the parent took of the
/// child's resident bytes separates that kill from every other
/// `SIGKILL`. A host that sets `RLIMIT_AS` inside the child fails an
/// allocation instead, and the Rust runtime then writes
/// [`ALLOCATION_FAILURE_TEXT`] and ends the child on `SIGABRT`. That
/// text is what this reads: an abort without it is abnormal.
#[cfg(unix)]
fn system_memory_kill(signal: i32, child_stderr: &[u8], largest_resident: Option<u64>) -> bool {
    if signal == libc::SIGKILL {
        return largest_resident.is_some_and(|bytes| bytes >= system_memory_kill_floor());
    }
    signal == libc::SIGABRT && holds(child_stderr, ALLOCATION_FAILURE_TEXT)
}

/// True when `haystack` holds `needle`.
#[cfg(any(unix, windows))]
fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Puts the child in a process group of its own (§109.2 rule 6).
///
/// A kill of that group reaches every process the child started, the C
/// compiler included.
#[cfg(unix)]
fn own_process_group(spawning: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the closure runs in the forked child, between `fork` and
    // `exec`. It calls `setpgid`, which is async-signal-safe, and
    // nothing else.
    let _ = unsafe {
        spawning.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        })
    };
}

/// Puts the child in a process group of its own (§109.2 rule 6).
///
/// Windows holds the child and every process it starts in the Job
/// Object the child creates, so the spawn needs nothing here.
#[cfg(not(unix))]
fn own_process_group(_spawning: &mut Command) {}

/// Kills the child and every process it started (§109.2 rule 6).
#[cfg(unix)]
fn kill_group(child: &mut Child) {
    let group = i32::try_from(child.id()).unwrap_or(0);
    // A process group id of 0 or 1 is never this child's, and the
    // negated form of either reaches processes the parent does not own.
    if group > 1 {
        // SAFETY: `kill` takes a negated process group id and a signal
        // number. The child leads its own group, so the id is the
        // child's own.
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
    // The group kill covers the child too. This second kill is what
    // ends a child whose `setpgid` the host refused.
    let _ = child.kill();
}

/// Kills the child and every process it started (§109.2 rule 6).
///
/// The child's Job Object carries `KILL_ON_JOB_CLOSE`, so the processes
/// it started end when the killed child's handle closes.
#[cfg(not(unix))]
fn kill_group(child: &mut Child) {
    let _ = child.kill();
}

/// Holds the spawned child until the parent reads its status
/// (§109.2 rule 6).
struct ChildGuard {
    /// The spawned child.
    child: Child,
    /// True after the parent read the child's status and the host
    /// reaped it.
    reaped: bool,
}

impl ChildGuard {
    /// Takes the spawned child.
    fn new(child: Child) -> Self {
        Self {
            child,
            reaped: false,
        }
    }

    /// Records that the child ended and the host reaped it.
    ///
    /// A reaped process id can name another process later, so the drop
    /// kills nothing after this call.
    fn reaped(&mut self) {
        self.reaped = true;
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        kill_group(&mut self.child);
        let _ = self.child.wait();
    }
}

/// Runs the watch loop's compile in a budgeted child (§109.2 rule 6).
///
/// The child runs `check` on the same entry under the profile, and its
/// output is discarded: the parent repeats the compile in process for
/// the reload session, and that compile reports the same diagnostics.
/// The child is what bounds the parser, so the parent never parses a
/// source the budgets refuse. The cost is one extra compile of each
/// edit, under the profile only.
pub(crate) fn guard_watch_compile(entry: &Path) -> Result<(), Failure> {
    let args = [
        OsString::from("--profile"),
        OsString::from("sandbox"),
        entry.as_os_str().to_os_string(),
    ];
    let mut out = Vec::new();
    let mut err = Vec::new();
    compile_in_child("check", &args, entry, &mut out, &mut err)?;
    Ok(())
}

/// How the child ended.
enum Stop {
    /// The child ended on its own.
    Exited(ExitStatus),
    /// The parent killed the child at the budget it names.
    Killed(Passed),
}

/// The budget a child passed under the parent's watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Passed {
    /// The child ran past the time budget.
    Time,
    /// The child held more than [`MEMORY_BUDGET_BYTES`].
    Memory,
}

impl Passed {
    /// The S026 message for this budget (§109.2 rule 6).
    fn message(self) -> &'static str {
        match self {
            Self::Time => TIME_MESSAGE,
            Self::Memory => MEMORY_MESSAGE,
        }
    }
}

/// Reads one pipe to its end.
///
/// A read error ends the pass-through with the bytes already read; the
/// child's exit status carries the outcome.
fn read_all<R: Read>(mut source: R) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = source.read_to_end(&mut bytes);
    bytes
}

/// The time budget of this run, or the test-only value.
fn time_budget() -> Duration {
    time_budget_of(std::env::var(TIME_BUDGET_VARIABLE).ok().as_deref())
}

/// The time budget a `TIME_BUDGET_VARIABLE` value asks for.
///
/// A value that is not a count of seconds, and a value over
/// [`TIME_BUDGET_CEILING_SECONDS`], leave the contract's budget.
fn time_budget_of(value: Option<&str>) -> Duration {
    let seconds = value
        .and_then(|text| text.parse::<u64>().ok())
        .filter(|seconds| *seconds <= TIME_BUDGET_CEILING_SECONDS)
        .unwrap_or(TIME_BUDGET_SECONDS);
    Duration::from_secs(seconds)
}

/// The instant the parent stops waiting on a child that it started at
/// `start`.
///
/// [`TIME_BUDGET_CEILING_SECONDS`] holds this addition inside
/// `Instant`'s range. A host that refuses the addition takes the
/// contract's own budget, and a host that refuses that too waits no
/// longer than `start`.
fn deadline_of(start: Instant, budget: Duration) -> Instant {
    start
        .checked_add(budget)
        .or_else(|| start.checked_add(Duration::from_secs(TIME_BUDGET_SECONDS)))
        .unwrap_or(start)
}

/// Waits for the child and kills it at the memory budget or at
/// `budget`, and answers the largest resident bytes it read
/// (§109.2 rule 6).
///
/// macOS refuses `RLIMIT_AS`, so the parent holds the memory budget
/// there: at every poll it reads the child's resident bytes, and it
/// kills a child whose reading passes the budget. On a host that sets
/// the limit inside the child, [`resident_bytes`] reads nothing and the
/// loop watches the time budget alone. The largest reading is what
/// tells the system's own memory kill from every other signal.
fn wait_within(child: &mut Child, budget: Duration) -> Result<(Stop, Option<u64>), Failure> {
    let deadline = deadline_of(Instant::now(), budget);
    let pid = child.id();
    let mut largest_resident = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((Stop::Exited(status), largest_resident)),
            Ok(None) => {}
            Err(error) => {
                return Err(Failure::usage(format!(
                    "wait for the compile child: {error}"
                )))
            }
        }
        let reading = resident_bytes(pid);
        largest_resident = largest_resident.max(reading);
        if over_budget(reading, MEMORY_BUDGET_BYTES) {
            return Ok((kill_child(child, Passed::Memory), largest_resident));
        }
        if Instant::now() >= deadline {
            return Ok((kill_child(child, Passed::Time), largest_resident));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Kills the child's process group and answers the budget it passed.
fn kill_child(child: &mut Child, passed: Passed) -> Stop {
    kill_group(child);
    let _ = child.wait();
    Stop::Killed(passed)
}

/// True when a reading of `resident` bytes passes `budget`.
///
/// The answer is false where the parent reads nothing, because the
/// limit the child set holds the budget there.
fn over_budget(resident: Option<u64>, budget: u64) -> bool {
    resident.is_some_and(|bytes| bytes > budget)
}

/// The resident bytes of the process `pid`, where the parent holds the
/// memory budget (§109.2 rule 6).
///
/// macOS refuses `RLIMIT_AS` (measured `EINVAL`), so the parent reads
/// the child's `ri_resident_size` through `proc_pid_rusage`.
#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> Option<u64> {
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: `rusage_info_v0` is plain data, and all zeros is one
    // value of it. `proc_pid_rusage` overwrites every field it reports.
    let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
    // SAFETY: the flavor selects `rusage_info_v0`, and the pointer is
    // the address of one live value of that type, as the C call takes
    // it.
    let read = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V0,
            std::ptr::from_mut(&mut info).cast::<libc::rusage_info_t>(),
        )
    };
    (read == 0).then_some(info.ri_resident_size)
}

/// The resident bytes of the process `pid`, where the parent holds the
/// memory budget (§109.2 rule 6).
///
/// Every other host sets the limit inside the child, so the parent
/// reads nothing.
#[cfg(not(target_os = "macos"))]
fn resident_bytes(_pid: u32) -> Option<u64> {
    None
}

/// One S026 at the entry file for a child that passed a budget.
///
/// The parent holds no parsed source, so the render carries the header
/// and the location line and no snippet.
fn budget_stop(entry: &Path, message: &str) -> Failure {
    let diagnostic = Diagnostic::new(
        RuleCode::S026,
        message,
        Pos::new(entry.to_string_lossy(), 1, 1),
    );
    Failure::rejection(render_diagnostics(&[], &[diagnostic]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_private_flag_leaves_the_arguments_the_subcommands_read() {
        let kept: Vec<OsString> = ["check", "--profile", "sandbox", "a.ts"]
            .iter()
            .map(OsString::from)
            .collect();
        let mut args = kept.clone();
        assert_eq!(take_role_of(&mut args, false).ok(), Some(Role::Parent));
        assert_eq!(args, kept);

        args.push(OsString::from(CHILD_FLAG));
        assert_eq!(take_role_of(&mut args, true).ok(), Some(Role::Child));
        assert_eq!(args, kept);
    }

    /// §109.2 rule 6: the child runs only when the parent marks it, so
    /// the private flag from a command line is a usage error.
    #[test]
    fn the_private_flag_with_no_marker_is_a_usage_error() {
        for command in ["check", "build", "run", "bind", "emit"] {
            let mut args: Vec<OsString> = [command, CHILD_FLAG, "a.ts"]
                .iter()
                .map(OsString::from)
                .collect();
            let failure = take_role_of(&mut args, false).expect_err("the flag has no marker");
            assert_eq!(failure.code, USAGE_ERROR);
            assert_eq!(failure.message, UNMARKED_CHILD_MESSAGE);
            assert!(!failure.verbatim);
            // The flag is private, so the message never names it.
            assert!(!failure.message.contains(CHILD_FLAG));
        }
    }

    #[test]
    fn only_the_parent_under_the_profile_spawns() {
        assert!(spawns(Role::Parent, Profile::Sandbox));
        assert!(!spawns(Role::Child, Profile::Sandbox));
        assert!(!spawns(Role::Parent, Profile::default()));
        assert!(!spawns(Role::Child, Profile::default()));
    }

    /// §109.2 rule 6: the child holds itself to the budget, or the host
    /// refused the limit.
    ///
    /// The two facts are derived apart: the function reports what it
    /// did, and `getrlimit` reads what the host holds. The soft limit
    /// this test set is restored, so the limit reaches no other test.
    #[cfg(unix)]
    #[test]
    fn the_child_reports_the_limit_the_host_holds() {
        let mut before = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `getrlimit` writes one `rlimit` through the pointer.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut before) }, 0);

        let outcome = apply_memory_budget();
        let mut after = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `getrlimit` writes one `rlimit` through the pointer.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut after) }, 0);
        // SAFETY: `setrlimit` reads one `rlimit` through the pointer.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &before) }, 0);

        let expected = budgeted_soft_limit(before.rlim_max);
        match outcome {
            MemoryBudget::Set => assert_eq!(after.rlim_cur, expected),
            MemoryBudget::Refused => assert_eq!(after.rlim_cur, before.rlim_cur),
        }
        println!(
            "RLIMIT_AS: {outcome:?}, budget {MEMORY_BUDGET_BYTES}, soft {}",
            after.rlim_cur
        );
    }

    /// §109.2 rule 6: the parent reads the child's resident bytes where
    /// the host refuses `RLIMIT_AS`, and reads nothing where the child
    /// sets its own limit.
    ///
    /// The two facts are derived apart: the budget is this module's
    /// number, and the bytes are the kernel's reading of this live
    /// process. The zero budget is the firing control, because every
    /// live process holds more than zero bytes.
    #[test]
    fn the_parent_reads_the_resident_bytes_of_a_process() {
        let mine = std::process::id();
        if cfg!(target_os = "macos") {
            let bytes = resident_bytes(mine).expect("read the resident bytes of this process");
            assert!(bytes > 0);
            assert!(over_budget(resident_bytes(mine), 0));
            assert!(!over_budget(resident_bytes(mine), MEMORY_BUDGET_BYTES));
            println!("resident bytes of the test process: {bytes}");
        } else {
            assert_eq!(resident_bytes(mine), None);
            assert!(!over_budget(resident_bytes(mine), 0));
        }
    }

    /// The kill of each budget carries its own S026 message.
    #[test]
    fn each_budget_names_its_own_stop() {
        assert_eq!(Passed::Memory.message(), MEMORY_MESSAGE);
        assert_eq!(Passed::Time.message(), TIME_MESSAGE);
    }

    /// The test-only variable replaces the budget, and the contract's
    /// number stands with no variable, with a value that is not a count
    /// of seconds, and with a value over the ceiling.
    #[test]
    fn the_time_budget_reads_the_test_only_variable() {
        assert_eq!(time_budget_of(Some("2")), Duration::from_secs(2));
        assert_eq!(
            time_budget_of(Some("soon")),
            Duration::from_secs(TIME_BUDGET_SECONDS)
        );
        assert_eq!(
            time_budget_of(None),
            Duration::from_secs(TIME_BUDGET_SECONDS)
        );
        assert_eq!(
            time_budget_of(Some("18446744073709551615")),
            Duration::from_secs(TIME_BUDGET_SECONDS)
        );
        assert_eq!(
            time_budget_of(Some("86401")),
            Duration::from_secs(TIME_BUDGET_SECONDS)
        );
        assert_eq!(
            time_budget_of(Some("86400")),
            Duration::from_secs(TIME_BUDGET_CEILING_SECONDS)
        );
        // The reader takes the same path as the values above.
        assert_eq!(time_budget(), Duration::from_secs(TIME_BUDGET_SECONDS));
    }

    /// §109.2 rule 6: the deadline is a total function of the budget.
    ///
    /// The two facts are derived apart: the budget is a `Duration` the
    /// caller names, and the deadline is the host's own clock
    /// arithmetic. `Duration::MAX` is the firing control, because the
    /// unchecked addition of it ends the parent.
    #[test]
    fn the_deadline_holds_every_budget_a_caller_names() {
        let start = Instant::now();
        assert_eq!(
            deadline_of(start, Duration::from_secs(2)),
            start + Duration::from_secs(2)
        );
        let ceiling = Duration::from_secs(TIME_BUDGET_CEILING_SECONDS);
        assert_eq!(deadline_of(start, ceiling), start + ceiling);
        let held = deadline_of(start, Duration::MAX);
        assert!(held >= start);
        assert!(held <= start + Duration::from_secs(TIME_BUDGET_SECONDS));
    }

    /// §109.2 rule 6: the parent reads each end of the child as the
    /// contract names it.
    ///
    /// The two facts are derived apart: the classification is this
    /// module's, and the wait status is the host's own encoding of an
    /// exit code and a signal number.
    #[cfg(unix)]
    #[test]
    fn the_parent_reads_every_end_of_the_child() {
        use std::os::unix::process::ExitStatusExt;

        let exited = |code: i32| ExitStatus::from_raw(code << 8);
        assert_eq!(classify(exited(0), b"", None), Ok(SUCCESS));
        assert_eq!(classify(exited(1), b"", None), Ok(PROGRAM_ERROR));
        assert_eq!(classify(exited(2), b"", None), Ok(USAGE_ERROR));
        assert_eq!(
            classify(exited(101), b"", None),
            Err(String::from(
                "the compiler stopped abnormally (exit code 101)"
            ))
        );

        let signalled = ExitStatus::from_raw;
        assert_eq!(
            classify(signalled(libc::SIGTERM), b"", None),
            Err(String::from("the compiler stopped abnormally (signal 15)"))
        );
        assert_eq!(
            classify(signalled(libc::SIGSEGV), b"", None),
            Err(String::from("the compiler stopped abnormally (signal 11)"))
        );

        // The system's own memory kill, and every other `SIGKILL`.
        let floor = system_memory_kill_floor();
        assert_eq!(
            classify(signalled(libc::SIGKILL), b"", Some(floor)),
            Err(String::from(MEMORY_MESSAGE))
        );
        assert_eq!(
            classify(signalled(libc::SIGKILL), b"", Some(floor - 1)),
            Err(String::from("the compiler stopped abnormally (signal 9)"))
        );
        assert_eq!(
            classify(signalled(libc::SIGKILL), b"", None),
            Err(String::from("the compiler stopped abnormally (signal 9)"))
        );

        // A failed allocation under `RLIMIT_AS`, and every other abort.
        let failed = b"memory allocation of 8589934592 bytes failed\n";
        assert_eq!(
            classify(signalled(libc::SIGABRT), failed, None),
            Err(String::from(MEMORY_MESSAGE))
        );
        assert_eq!(
            classify(signalled(libc::SIGABRT), b"assertion failed\n", None),
            Err(String::from("the compiler stopped abnormally (signal 6)"))
        );
    }

    /// §109.2 rule 6: the parent reads each end of the child as the
    /// contract names it, on a host with no signals.
    ///
    /// The two facts are derived apart: the classification is this
    /// module's, and the exit code is the host's own encoding.
    /// `__fastfail`'s code is the firing control: the same code
    /// without the runtime's text is an abnormal end.
    #[cfg(windows)]
    #[test]
    fn the_parent_reads_every_end_of_the_child() {
        use std::os::windows::process::ExitStatusExt;

        /// The code `__fastfail` gives a Rust abort
        /// (`STATUS_STACK_BUFFER_OVERRUN`), as `status.code()` reads it.
        const FAST_FAIL: u32 = 0xC000_0409;

        let exited = ExitStatus::from_raw;
        assert_eq!(classify(exited(0), b"", None), Ok(SUCCESS));
        assert_eq!(classify(exited(1), b"", None), Ok(PROGRAM_ERROR));
        assert_eq!(classify(exited(2), b"", None), Ok(USAGE_ERROR));
        assert_eq!(
            classify(exited(101), b"", None),
            Err(String::from(
                "the compiler stopped abnormally (exit code 101)"
            ))
        );

        // A failed allocation under the Job Object's memory limit, and
        // every other end that carries the same code.
        let failed = b"memory allocation of 262144 bytes failed
";
        assert_eq!(
            classify(exited(FAST_FAIL), failed, None),
            Err(String::from(MEMORY_MESSAGE))
        );
        assert_eq!(
            classify(
                exited(FAST_FAIL),
                b"assertion failed
",
                None
            ),
            Err(String::from(
                "the compiler stopped abnormally (exit code -1073740791)"
            ))
        );
    }

    /// §109.2 rule 6: the parent kills the child on every path out of
    /// the compile, its own unwind included.
    ///
    /// The two facts are derived apart: the guard reports nothing, and
    /// the marker is the file system's own record of what ran. The
    /// firing control is the same child with no guard: it writes the
    /// marker.
    #[cfg(unix)]
    #[test]
    fn the_guard_kills_the_child_it_holds() {
        let marker = |name: &str| {
            std::env::temp_dir().join(format!("subscript-guard-{}-{name}", std::process::id()))
        };
        let start = |path: &Path| {
            let mut spawning = Command::new("/bin/sh");
            spawning
                .arg("-c")
                .arg(format!("sleep 0.3; : > '{}'", path.display()))
                .stdin(Stdio::null());
            own_process_group(&mut spawning);
            spawning.spawn().expect("start the marker process")
        };

        // The firing control: the child reaches its end and writes.
        let written = marker("control");
        let _ = std::fs::remove_file(&written);
        let mut control = start(&written);
        assert!(control.wait().expect("wait for the control").success());
        assert!(written.is_file(), "the control writes its marker");
        let _ = std::fs::remove_file(&written);

        // The guard drops before the child reaches its end.
        let killed = marker("killed");
        let _ = std::fs::remove_file(&killed);
        drop(ChildGuard::new(start(&killed)));
        // The drop waits for the child, so nothing writes after it.
        assert!(!killed.is_file(), "the killed child writes no marker");
        std::thread::sleep(Duration::from_millis(600));
        assert!(!killed.is_file(), "the killed child writes no marker");
    }

    /// The rendered line is the one §109.2 rule 6 names.
    #[test]
    fn a_budget_stop_renders_one_s026_at_the_entry_file() {
        let failure = budget_stop(Path::new("entry.ts"), MEMORY_MESSAGE);
        assert_eq!(failure.code, PROGRAM_ERROR);
        assert!(failure.verbatim);
        assert_eq!(
            failure.message,
            concat!(
                "error[S026]: the compiler passed its memory budget\n",
                " --> entry.ts:1:1\n",
                "error: 1 error(s)"
            )
        );
        let timed = budget_stop(Path::new("entry.ts"), TIME_MESSAGE);
        assert!(timed
            .message
            .starts_with("error[S026]: the compiler passed its time budget\n"));
    }
}
