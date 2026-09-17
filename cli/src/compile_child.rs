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
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use subscript_compiler::{render_diagnostics, Diagnostic, Pos, Profile, RuleCode};

use crate::{Failure, PROGRAM_ERROR, SUCCESS, USAGE_ERROR};

/// The private flag the parent adds to the child's arguments.
///
/// [`crate::execute`] removes it before any subcommand reads the
/// arguments, so no argument parser sees it and no usage text names it.
pub(crate) const CHILD_FLAG: &str = "--compile-child";

/// The environment variable that replaces the compile child's time
/// budget, in seconds (§109.2 rule 6).
///
/// A test sets it to reach the time-budget stop in seconds instead of
/// [`TIME_BUDGET_SECONDS`]. A value that is not a count of seconds
/// leaves the contract's budget in place.
pub const TIME_BUDGET_VARIABLE: &str = "SUBSCRIPT_COMPILE_TIME_BUDGET_SECONDS";

/// The time budget the parent gives the child, in seconds
/// (§109.2 rule 6).
pub(crate) const TIME_BUDGET_SECONDS: u64 = 300;

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
pub(crate) fn take_role(args: &mut Vec<OsString>) -> Role {
    let before = args.len();
    args.retain(|arg| arg != CHILD_FLAG);
    if args.len() == before {
        Role::Parent
    } else {
        Role::Child
    }
}

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
#[cfg(windows)]
pub(crate) fn apply_memory_budget() -> MemoryBudget {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
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
            LimitFlags: JOB_OBJECT_LIMIT_PROCESS_MEMORY,
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
/// stderr pass through unchanged. A child that passes the time budget,
/// dies by a signal, or exits with an allocation failure is one S026 at
/// `entry`.
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawning
        .spawn()
        .map_err(|error| Failure::usage(format!("start the compile child: {error}")))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| Failure::usage("the compile child has no stdout pipe"))?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| Failure::usage("the compile child has no stderr pipe"))?;

    // The readers drain both pipes for the whole wait, so a child that
    // writes more than one pipe buffer never blocks on the parent.
    let (stop, out_bytes, err_bytes) = std::thread::scope(|scope| {
        let reading_out = scope.spawn(move || read_all(child_stdout));
        let reading_err = scope.spawn(move || read_all(child_stderr));
        let stop = wait_within(&mut child, time_budget());
        (stop, reading_out.join(), reading_err.join())
    });
    let out_bytes =
        out_bytes.map_err(|_| Failure::usage("reading the compile child's stdout failed"))?;
    let err_bytes =
        err_bytes.map_err(|_| Failure::usage("reading the compile child's stderr failed"))?;
    stdout
        .write_all(&out_bytes)
        .and_then(|()| stdout.flush())
        .map_err(|error| Failure::usage(format!("write program stdout: {error}")))?;
    stderr
        .write_all(&err_bytes)
        .and_then(|()| stderr.flush())
        .map_err(|error| Failure::usage(format!("write compiler output: {error}")))?;

    match stop? {
        Stop::Killed(passed) => Err(budget_stop(entry, passed.message())),
        Stop::Exited(status) => match status.code() {
            Some(0) => Ok(SUCCESS),
            Some(1) => Ok(PROGRAM_ERROR),
            Some(2) => Ok(USAGE_ERROR),
            // A child that ends on a signal reports no code, and an
            // allocation failure aborts. Either is the memory budget:
            // the limit the child set, or the system's own.
            _ => Err(budget_stop(entry, MEMORY_MESSAGE)),
        },
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
    Exited(std::process::ExitStatus),
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
fn time_budget_of(value: Option<&str>) -> Duration {
    let seconds = value
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(TIME_BUDGET_SECONDS);
    Duration::from_secs(seconds)
}

/// Waits for the child and kills it at the memory budget or at
/// `budget` (§109.2 rule 6).
///
/// macOS refuses `RLIMIT_AS`, so the parent holds the memory budget
/// there: at every poll it reads the child's resident bytes. On a host
/// that sets the limit inside the child, [`resident_bytes`] reads
/// nothing and the loop watches the time budget alone.
fn wait_within(child: &mut Child, budget: Duration) -> Result<Stop, Failure> {
    let deadline = Instant::now() + budget;
    let pid = child.id();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Stop::Exited(status)),
            Ok(None) => {}
            Err(error) => {
                return Err(Failure::usage(format!(
                    "wait for the compile child: {error}"
                )))
            }
        }
        if over_budget(pid, MEMORY_BUDGET_BYTES) {
            return Ok(kill_child(child, Passed::Memory));
        }
        if Instant::now() >= deadline {
            return Ok(kill_child(child, Passed::Time));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Kills the child and answers the budget it passed.
fn kill_child(child: &mut Child, passed: Passed) -> Stop {
    let _ = child.kill();
    let _ = child.wait();
    Stop::Killed(passed)
}

/// True when the process `pid` holds more than `budget` resident bytes.
///
/// The answer is false where the parent reads nothing, because the
/// limit the child set holds the budget there.
fn over_budget(pid: u32, budget: u64) -> bool {
    resident_bytes(pid).is_some_and(|bytes| bytes > budget)
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
        assert_eq!(take_role(&mut args), Role::Parent);
        assert_eq!(args, kept);

        args.push(OsString::from(CHILD_FLAG));
        assert_eq!(take_role(&mut args), Role::Child);
        assert_eq!(args, kept);
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
            assert!(over_budget(mine, 0));
            assert!(!over_budget(mine, MEMORY_BUDGET_BYTES));
            println!("resident bytes of the test process: {bytes}");
        } else {
            assert_eq!(resident_bytes(mine), None);
            assert!(!over_budget(mine, 0));
        }
    }

    /// The kill of each budget carries its own S026 message.
    #[test]
    fn each_budget_names_its_own_stop() {
        assert_eq!(Passed::Memory.message(), MEMORY_MESSAGE);
        assert_eq!(Passed::Time.message(), TIME_MESSAGE);
    }

    /// The test-only variable replaces the budget, and the contract's
    /// number stands with no variable and with a value that is not a
    /// count of seconds.
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
        // The reader takes the same path as the two above.
        assert_eq!(time_budget(), Duration::from_secs(TIME_BUDGET_SECONDS));
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
