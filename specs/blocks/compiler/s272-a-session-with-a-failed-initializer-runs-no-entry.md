<!-- §272 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 272. A session with a failed initializer runs no entry

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R08.
The owner selected the form on 2026-11-13: the session refuses each
entry call until a reload runs the initializer again from the start.

Problem, read at `4453fff3`. `ReloadSession::new_capturing_initializer_trap`
returns the session also when the module initializer traps
(`codegen/src/reload.rs`), and `subscript run --watch` keeps that
session (`cli/src/watch.rs`, `specs/tracking/cli.md`). The next host
call clears the trap (`prepare_call`, §8.2) and runs the entry. No
state of the session records that the initializer stopped. So a global
whose initializer comes after the trap holds zero or null, a value that
no initializer of the program makes, and the entry reads it. A reload
keeps the globals and does not run the initializer (§8.2), so a reload
that fixes the trap does not make the globals either.

### 272.1 Rules

1. A session records whether its initializer completed.
2. While the initializer has not completed, each entry call
   (`call_main`, `run_main`, `call_export`, `call_export_with`, and an
   async step that runs script code) returns an error that names the
   initializer trap, and runs no script code.
3. A reload of a session whose initializer has not completed, with code
   that the checker accepts, makes the global block again with no value
   of the old block, and runs the initializer from the start. Each side
   effect of the initializer runs again, such as `print`. If the
   initializer traps again, rules 1 and 2 hold.
4. A reload of a session whose initializer completed keeps the present
   rule of §8.2: the globals stay, and the initializer does not run.
5. `subscript run --watch` reports the refused call of rule 2 as it
   reports a trap, and waits for an edit.

### 272.2 Acceptance

1. Red first, at the pin: a unit test makes a session from a program
   whose initializer throws before a later global `const g = 7`, and
   calls an export that prints `g`. At the pin the call succeeds and
   prints `0`. The tracking note records it.
2. After the change, the call returns the error of rule 2, and prints
   nothing.
3. A reload with the trap removed runs the initializer: its `print`
   runs again, and the export prints `7`.
4. A control: a session whose initializer completed keeps a changed
   global across a reload, as now.
5. A watch test: an initializer trap, then an edit that fixes it, runs
   the fixed program with the initialized globals.
6. No corpus entry or golden changes.

### 272.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each public method of `ReloadSession`
that runs script code; each one refuses first, and a refusal changes no
state. A rebuild drops the old Context, so the allocations, the async
jobs, the registrations, and the file requests of the failed
initializer go with it. A reload that the checker rejects keeps the old
session.

1. MINOR, realistic, fixed: the check for live workers ran before the
   rebuild, so an initializer that started a worker and then trapped
   made each reload fail. The rebuild now runs first; the drop of the
   Context joins each worker.
2. MINOR, realistic, fixed: the rebuild dropped the output of the old
   session that the host did not take. The new session holds it before
   the output of the new initializer.
3. MINOR, fixed: the test of rule 3 did not show that the old
   allocations are released. It now compares the live allocations with
   a fresh session.

### 272.4 Open

None.
