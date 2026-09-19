<!-- §109 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 109. The sandbox profile

*(Owner decision 2026-09-16.)* Origin: plan Rev 3. Evidence:
`specs/tracking/sandbox-tier-2026-09-16.md`. Phase: P26.

Problem: a host that runs content it did not write (mods, shared
levels, plugins) had no execution form. Invariant 6 now reads
"Scripts are trusted, except under the sandbox profile". This section
is the profile.

The profile is a **compile profile**, not a tier. The checker narrows
the accepted language. The lowering emits two intrinsics that every
tier executes. The runtime holds three host-set limits. A program
that does not select the profile gets no new instruction and no new
check.

### 109.0 What the profile guarantees

*(Added 2026-09-17, after a policy review that asked for outcomes,
not only prohibitions.)* The rules in 109.1 to 109.5 exist to hold
the four statements below. A rule that does not serve one of them is
not a profile rule.

1. **Reach.** An accepted profile program reaches only the functions
   its mirror declares and the memory its own Context allocated. It
   holds no address it did not receive from the runtime or the host.
2. **Stop.** The program stops with an ordinary trap before any
   host-set limit is exceeded: the allocation quota before the bytes
   exist, the stack budget before the next frame, the interrupt at
   the next checkpoint. The Context survives and the host reads the
   trap.
3. **Return.** After the host sets the interrupt, control returns to
   the host within bounded work: the straight-line code between two
   checkpoints, plus one runtime operation in flight. A runtime
   operation has no interior checkpoint; its work is a polynomial in
   its input and output sizes, and both are under the quota. Regular
   expressions carry their own work budget (§23).
4. **Compile.** On every source within S026's limits, the compiler
   returns a diagnostic or an accepted program. This project's own
   stages do that in work bounded by a polynomial in the source
   size, on a thread whose stack it sizes. The parser is external,
   and its work and memory on a hostile shape are not bounded by
   construction (M10, M13). The CLI therefore holds the guarantee
   by a process boundary: under the profile it compiles in a child
   process with a memory budget and a time budget, and a child that
   passes either budget is S026. A host that embeds the compiler
   crate runs it the same way (109.2 rule 6).

**What the host supplies.** Each guarantee rests on a fact only the
host holds. The host tutorial states each one where the host acts.

- The thread that runs script has more stack than the budget plus
  the headroom 109.4 rule 3 names.
- Every function in the mirror is part of the trusted boundary. It
  validates the arguments a script controls (pointer and count pairs,
  handles, indices, lengths), bounds its own work or is excluded from
  guarantee 3 by the host's own statement, and does not block.
- The profile arms no interrupt. A host that needs guarantee 3 arms
  one, or accepts that a program under the profile runs until it
  returns or traps.
- Same-process execution trusts the compiler, the generated code, and
  the runtime to be memory-safe. A host that must contain a defect in
  those components adds an isolation boundary of its own, such as a
  separate process. The profile does not provide one.

**What is excluded, by name.**

- `Context.collect()` has no interior checkpoint. Its work is
  proportional to the live set plus the dead set, which the quota
  bounds; it is not interruptible. A host that needs a latency bound
  calls collect at its own boundary (109.8a) and sizes the quota to
  the collect it can afford.
- A host function's work is the host's (above).
- The quota bounds the bytes the Context reserves for the program.
  It does not bound the host's own allocations, or the compiler's.

**Memory, precisely.** The quota charges the bytes the allocator
reserves for an allocation: the payload rounded to its size class
plus the block header in the arena mode, and the payload plus the
header plus the per-allocation record in the exact-size mode. Process
memory attributable to the Context is then the quota plus a fixed
overhead, whatever the allocation sizes. The host reads the charged
bytes through `subscript_rt_ctx_charged_bytes` (§18.2d) and paces on
that figure (109.8a). Reserved bytes are a tier fact, as `live_bytes`
is (§18.2d): a 64 MiB quota holds 6,100,800 payload bytes of 8-byte
objects in the exact-size mode and 33,554,432 in the arena mode, and
about 66 MB of 4 KiB objects in both. *(Measured in the M6 round at
`52373a9`, before the charge: with 8-byte objects a 64 MiB quota held
1,023,410,200 bytes in the exact-size mode, 15.25x, and 134,348,816
in the arena mode, 2.00x; after the charge the worst case is 1.04x.
The estimate this paragraph first carried, 5x and 11x, was wrong.)*

### 109.1 Selection

1. The CLI accepts `--profile sandbox` on `check`, `build`, and
   `run`. The default profile has no name and no flag.
2. `CheckOptions` carries the profile, and the checker reads it for
   109.2. The checked HIR module carries the profile it was checked
   under, so every consumer of HIR reads the same fact: the LIR
   lowering for 109.3, and each tier runner for the 109.5 defaults.
   No runner or lowering takes the profile as a second parameter.
   *(Amended 2026-09-16, before round 2: the first text named
   `LowerOptions` as a second carrier. One form carries the fact,
   core principle 8.)*
3. A corpus entry selects the profile with the header line
   `// profile: sandbox`. Every harness that compiles an entry reads
   the header and passes the profile to both options.
4. A program under the profile is a program of the accepted
   language. Every rule below removes a form or adds a check.
   Nothing is added to the surface, so the `tsc` gate is unchanged.

### 109.2 Compile-time rules

Each rule is one `RuleCode`. Each has one reject entry (109.7). Each
diagnostic names the profile in its message.

| Code | Rejects |
|---|---|
| S023 | `Context.free`. Memory is allocate-only. `Context.collect()` stays callable: a rejection adds no safety, and its work is bounded by the quota (109.0, excluded from guarantee 3). |
| S027 | A function whose frame is over 65,536 bytes, under the profile. The checker already sizes every frame (`MAX_FRAME_BYTES`); the profile lowers the limit so that the stack check at `Enter` sees at most one bounded frame past the budget. |
| S024 | `Context.fromBytes`, always. `Context.bytesOf` and `Context.bytesInto` stay accepted: the default profile already rejects a layout with a handle, a reference, or a string (S100, the value-class whitelist), so no profile rule is needed. |
| S025 | `Worker.spawn`, `Inbox`, and `Outbox`. |
| S026 | Source over a byte limit, before the parser runs: more than 131,072 bytes in one file, or more than 8,388,608 bytes in one program (every file the entry imports, mirrors excluded). The count is over bytes and needs no lexer. The parser's stack holds the deepest nesting a file of that size can spell (109.2a). The exact nesting bound is the checker's guard (rule 2), after the parse. |

*(Amended 2026-09-17, after an external review.)* The first text
counted bytes with no lexing and claimed the count over-approximates
the syntactic depth. It does not: a closer inside a comment
decrements the count, so `(/*)*/` repeated cancels the scan. Measured
at `5d288f5` with the release CLI: 257 such levels check clean under
the profile; 2,000 abort the process with a main-thread stack
overflow, in the warning walk that runs after the checker thread
returns. Every stage after the checker that recurses over the tree
(`check_warnings`, the LIR lowering, the emitters) inherits the
checker's nesting bound (rule 2) and runs on the compile thread
(rule 3). The default profile keeps no limit and is unchanged.
*(The token count this paragraph once named is retired, rule 5.)*

*(Amended 2026-09-16, after round 1.)* The first text assigned S019
to S022. §99.3 retires S019 and forbids its reuse, so the codes are
S023 to S026. The first text gave S024 a second clause over
`bytesOf` and `bytesInto` layouts with a handle. Round 1 measured
that the default profile rejects every such layout at the
declaration and at the call, so the clause had no program and is
removed (core principle 9).

**The compiler runs on its own thread, and the checker bounds the
tree.** Round 1 measured that the parser and the checker recurse
once per nesting level: a debug test thread of 2 MiB overflows at
depth 66, and the release CLI on the main thread runs depth 257.
`check_program_with` runs the parse and the check on a thread it
spawns with a 64 MiB stack, and joins it. The depth capacity is then
a compiler fact, not the caller's thread. S026's limit of 256 is
under that capacity on every harness thread, so the depth entry is
constructible and the §90 mutation sweep returns on every mutation
of it. The default profile keeps no depth limit.

*(Amended 2026-09-17, after the adversarial review.)* Brackets are
not the only nesting. Measured at `5d288f5` with the release CLI
under the profile: `Array<Array<…>>` at depth 2,000 (14 KB), a chain
of 60,000 `!`, and a chain of 40,000 `? :` (480 KB) each abort the
process with a main-thread stack overflow, in the stages that run
after the checker thread returns. A chain of 30 `!` takes over 25 s
in the checker, doubling per operator. Three rules close the class:

1. **The checker visits each syntax node once.** A chain of n
   operators costs O(n). The unary-chain cost is a checker defect,
   fixed at its site, with a test that bounds the time of a 200-deep
   chain.
2. **S026's third limit is a nesting limit on the tree the checker
   walks.** One depth guard type, entered at every recursive
   descent in the checker (expressions, types, statements, patterns);
   a depth over 256 is S026 at that node, under the profile only.
   The HIR the checker emits is therefore bounded, and every later
   stage (`check_warnings`, the LIR lowering, the emitters) inherits
   the bound without a guard of its own. No count runs before the
   parser except the byte limit (rule 5).
3. **The whole pipeline runs on the compile thread.** The compile
   thread of 109.2a is the thread of the whole compile: parse, check,
   warnings, lowering, and emission, in the CLI and in every codegen
   runner. Nothing that recurses over the tree runs on the caller's
   thread. `check_program_with` spawns that thread itself when the
   caller is not already on it, so a host that embeds the compiler
   crate gets the bound from the API, not from a wrapper; a caller
   already on the thread runs inline. Every public entry that parses
   (`parse_import_specifiers` included) spawns the same way. *(Amended 2026-09-17, second
   Phase Review: the round-4 form left the spawn to the callers.)*
4. **Work and output are budgeted.** Under the profile the checker
   counts the nodes it visits and the type instances it creates (one
   unit at each of the four instantiation sites, and the body's nodes
   through the descent); over 16,777,216 it reports S026 and stops.
   The nesting guard's type descent covers a type-parameter
   constraint and a type-parameter default as it covers an
   annotation; the checker resolves neither, so the guard walks the
   type nodes and creates no instance. The LIR lowering counts
   the instructions it emits; over 4,194,304 it reports S026 and
   stops. Nesting bounds depth; these two bound the width that
   instantiation and expansion can add.

The parser runs before rule 2 can reject, so its own tolerance is
measured, not assumed: for each nesting construct the grammar has
(type arguments, prefix operators, conditional expressions,
conditional types, assignment and exponent chains, member and call
chains, binary chains, template substitutions, unions, array-type
suffixes), the deepest source under 1,048,576 bytes runs through the
parser alone on the compile thread. A construct that overflows there
gets a token-level proxy limit before the parser, recorded in 109.2a
with its number. The stack size of the compile thread is set from
that measurement.

5. **The parser's entry owns the S026 byte check.** Under the
   profile, the one lexer constructor in `parse.rs` refuses a source
   over the byte limit. No caller can parse a file that check did not
   admit. *(Amended 2026-09-17, third Phase Review. The class "the
   parser runs on a source the scan did not bound" was raised three
   times: `program_loader` parsed on the caller's thread; it parsed
   before the scan; and a lexer with no parser reads a regex literal
   as division, so a quote inside one desyncs the token and bracket
   count for the rest of the file. A count over tokens cannot be
   exact without the parser. The token limit and the pre-parse
   bracket depth are retired. The byte limit is exact and total, and
   the parser's stack is sized to it.)*

6. **The CLI compiles in a budgeted child under the profile.**
   `check`, `build`, `run`, and the watch loop's compile run the
   compile in a child `subscript` process. The child has a memory
   budget and a time budget of 300 s. The memory budget is the heap a
   compile takes: 4,294,967,296 bytes unoptimized and 2,147,483,648
   optimized. Each host builds its own limit from that one number,
   because each host's limit counts a different thing. On Linux the
   child sets `RLIMIT_AS` to the heap plus the compile thread's stack
   reservation on its first line. On Windows the child creates a Job
   Object with a process memory limit of the heap and joins it on its
   first line; the job kills every process in it when the parent's
   handle closes. Linux turns the budget into a failed allocation:
   the Rust runtime writes "memory allocation of n bytes failed" and
   ends the child. Windows fails a heap allocation or a stack commit,
   and the paragraph below names both ends. On macOS
   `setrlimit`
   refuses `RLIMIT_AS` (measured `EINVAL`), so the parent reads the
   child's resident bytes at every 10 ms poll through
   `proc_pid_rusage` and kills a child over the budget. The parent
   kills a child past the time budget. A kill reaches the child's
   whole process group, so a C compiler the child started dies with
   it. The child runs only when the parent marks it: the private
   flag without the parent's environment marker is a usage error.
   The outcome is one line at the entry file. A child the parent
   killed for memory, or that the system killed for memory, reads
   "the compiler passed its memory budget". A child the parent
   killed for time reads "… its time budget". A child that stopped
   any other way reads "the compiler stopped abnormally (signal n)"
   or "(exit code n)", and that line is S026 too. Exit codes 0, 1,
   and 2 pass through. The child's diagnostics pass through
   unchanged. The parent kills the child on its own panic. Under the
   default profile nothing spawns. The watch loop checks each edit
   in a budgeted child first and then compiles in process for the
   live session. An edit that lands between the two compiles is
   parsed in process; that window is the operator's own loop.
   *(Amended 2026-09-18: the Windows classification read the budget's
   own end as an abnormal exit code. Measured on
   `x86_64-pc-windows-msvc`: the Job Object limit fails an
   allocation, and `__fastfail` then ends the child with exit code
   -1073740791, which no signal describes.)*
   *(Amended 2026-09-17, fifth Phase Review: the first classification
   read every abnormal end as the memory budget; a kill reached the
   direct child only; the flag was reachable from the command line.)*
   *(Added 2026-09-17, fourth Phase Review: a chain of 65,476 same
   labels in 130,990 bytes takes over 10 GB in the parser's
   duplicate-label path and is killed by the system, with no
   diagnostic; the parser is external, so the bound is a process.
   Superseded 2026-09-19: that path was quadratic and the fork fixed
   it. `parse_labelled_stmt` built one error for each earlier live
   copy of the label, so n nested duplicate labels made n(n-1)/2
   errors; 499 labels made 124,251. The same 130,990-byte source now
   takes 93,192,192 bytes and 0.76 s in the parser, and the whole
   profile check of it takes 1.65 s. The bound is still a process:
   the fix removes one input from the set this rule covers, not the
   rule. Fork commit `affcb6ee`.)*

   **The memory budget is the heap, and each host builds its own
   limit from it.** *(Added 2026-09-19. It replaces two paragraphs of
   the same date that made the budget the reservation plus the heap.
   That sum described one host of three.)* The compile thread's stack
   reservation is the reason the hosts differ. Linux bounds address
   space, and the reservation is address space, so the Linux limit is
   the heap plus the reservation. Windows bounds committed bytes, and
   a reservation commits nothing, so the Windows limit is the heap
   alone. macOS bounds resident bytes, and the reservation is never
   resident, so the poll compares the largest resident reading
   against the heap alone. The floor that separates a system memory
   kill from every other `SIGKILL` is one half of the heap.
   *(Measured 2026-09-19 on `x86_64-pc-windows-msvc`: a Job Object
   commit limit of 209,715,200 bytes does not refuse a thread that
   reserves 8,589,934,592 bytes of stack. That thread starts and
   touches 8 MiB of its stack, and the heap then fails at 256 MiB of
   commit.)*

   **A Windows budget stop has two ends, because the limit counts the
   stack the compile thread commits.** *(Added 2026-09-19.)* The
   reservation commits nothing at the start, and each recursion level
   commits the stack pages it touches. A Windows child that passes
   the budget therefore ends in one of two ways. A heap allocation
   fails: the runtime writes "memory allocation of n bytes failed",
   and `__fastfail` ends the child with exit code -1073740791. A
   stack commit fails: the runtime writes "thread '<name>' has
   overflowed its stack", and the host ends the child with exit code
   -1073741571, `STATUS_STACK_OVERFLOW`. The parent reads both ends
   as the memory budget. §109.2a sizes the reservation so that no
   source inside S026's byte limit overflows it, so the limit the
   parent set is the only other cause of an overflow. *(Measured
   2026-09-19: the 65,476-label source under a commit limit of
   33,554,432 through 805,306,368 bytes ends in the stack overflow.
   At 872,415,232 bytes the compile completes, and the parser's own
   diagnostic arrives.)*

   **A test replaces the memory budget by its heap.** *(Added
   2026-09-19.)* Each budget has a test-only environment variable,
   because the stop has no other deterministic source: a test that
   waits for a real 4 GiB heap waits for a host that can hold one.
   The variable takes the heap, so one value in a test composes on
   every host the way the contract's own heap does. A value of zero,
   and a value that is not a count of bytes, leave the contract's own
   heap in place, as an unreadable time-budget value does. A test
   derives its value from the measured demand of its own source: over
   what a clean source needs, and under what the source under test
   needs.

*(Measured 2026-09-17, security round 2, at `52373a9`.)* The
exponential `!` chain was the warning walk: the `Unary`/`Cast` arm of
`warn_w002_expr_uses` walked its operand and then fell through to the
loop over `children()`, so each level visited its child twice. The
rule for that walker: an arm that walks a child returns. The
main-thread abort at the pin was `program_loader::parse_import_specifiers`,
which parsed each file on the caller's thread to read its imports
before the checker's thread existed. Both are closed by rules 1 and
3. The nesting guard enters at three sites: the expression descent,
the type descent, and the statement descent. Patterns take no guard
because every pattern level opens a bracket; a class does not nest;
a function body nests only through a lambda, which is an expression.
The depth counts every node on the path, the enclosing statement and
the leaf included, so a chain of n operators reaches depth n + 2, an
arrow level is two, and the deepest accepted shapes are: 254 nested
type arguments, 254 prefix operators, 254 conditional expressions,
127 nested arrow bodies, and 255 parentheses (the bracket count and
the nesting guard are two limits of 256 that meet one level apart on
a parenthesis chain). Neither budget of rule 4 is reachable from a
source inside S026's limits: the largest work a 1 MiB source produced
was 187,498 units against 16,777,216, so both budgets are safety
nets, tested through a test-only budget. The LIR lowering stops at
its budget, and the CLI renders that stop as S026: `LowerError`
carries the rule code (closed in the docs round).

**M10, decided.** *(2026-09-17.)* Measured inside every S026 limit,
release: `<i32>` type assertions × n is O(n²) in the SWC parser
(16,000: 24.5 s; 26,190 at the byte limit: 140 s); a nested generic
call is superlinear there (the fourth review measured 112 s at 4,000
levels on its spelling; round 9 measured `f<A<…A<1+1>…>>(1)` at
6.2 s for 4,000 and the time budget at 43,664); a same-label chain
`a:a:…` is superlinear in memory in the parser's duplicate-label
path (32,000: 10.3 GB; 65,476: 10.4 GB, the memory budget); a
numeric literal of n digits is O(n²) in the SWC lexer. All are the parser's, and this project does
not patch the parser. The bound is the budgeted child of rule 6: a
compile that passes 300 s or the memory budget is S026, and the
tutorial states this beside the other host facts. The other three shapes (decorators on one line, one per
line, and labels) were this compiler's diagnostic renderer:
`source_line` scanned the file from byte zero for every item, and the
snippet copied the whole source line per item, so 32,000 diagnostics
on one 160 KB line rendered 7.69 GB. Rule: **the renderer indexes the
lines of a file once, writes a window of at most 240 bytes around
the byte of the column's character (the column is a character
count, §5), cut to UTF-8 boundaries, and renders at most 200 items;
the summary line carries the total.** *(Measured after the fix:
640,000 items on one 160 KB line render in 19 ms and 59,986 bytes;
800,000 label items in 31 ms.)*

### 109.2a The compile thread and the byte limit

*(Rewritten 2026-09-17, third Phase Review.)* The parser recurses
once per nesting level. The stack cost of one level, measured per
construct on the deepest source of each (M7), and the bytes one
level needs:

| Construct | Bytes per level, optimized | Source bytes per level |
|---|---|---|
| parenthesis | 6,750 | 1 |
| type arguments | 5,313 | 2 |
| template substitution | 5,249 | 2 |
| assignment chain | 2,832 | 2 |
| conditional expression | 2,512 | 2 |
| prefix operator | 152 | 1 |

The worst product of cost and density is the parenthesis: one byte
per level at 6,750 bytes of stack optimized and 31,151 unoptimized
(M11; the unoptimized ratio is 4.61x for a parenthesis, not the
3.02x of a type argument). The build selector in the code is
`cfg!(debug_assertions)`: the "unoptimized" row is the Cargo `dev`
and `test` profiles, the "optimized" row is `release`. A custom
profile that pairs `opt-level = 0` with `debug-assertions = false`
gets the smaller stack; the gate runs no such profile. A file of `SOURCE_BYTE_LIMIT` bytes can therefore
need `SOURCE_BYTE_LIMIT × 6,750` bytes of stack, and the compile
thread's stack must hold that with a margin of at least 1.5, in each
build:

| Build | Bytes per level | Stack | Worst file (131,072 bytes) | Margin |
|---|---|---|---|---|
| optimized | 6,750 | 2,147,483,648 | 884,736,000 | 2.43 |
| unoptimized | 31,151 | 8,589,934,592 | 4,083,023,872 | 2.10 |

*(Amended 2026-09-17, after M11: the first table set the unoptimized
cost at 20,385 and the stack at 4 GiB, a measured margin of 1.05.
The stack is 8 GiB; the reservation commits nothing untouched, as
M9 measured for 1 GiB and 4 GiB, and M12 measures it for 8 GiB.)*

A test derives the margin from the three constants of the build it
runs in and fails under 1.5. The byte limit is a contract number,
the same in every build. M11 re-measures every construct of M7 on a
file at the byte limit in both builds; a construct that overflows
moves the stack or the limit, and the round records it. The Windows
and Linux gate hosts run the same (§100).

The first form set 1,048,576 bytes per file with a token limit of
131,072 and a bracket depth of 256, both counted over a lexer with
no parser. A file of 1,048,576 bytes at 6,750 per level needs 7 GB
of stack, and the two counts were not exact (rule 5). 131,072 bytes
is about 3,000 lines; a larger program splits into files, and the
program limit of 8,388,608 bytes holds 64 of them.

**A refused spawn is a rejection under the profile.** If the compile
thread cannot be created, `check_program_with` under the profile
reports S026 "the compile thread is unavailable" and checks nothing,
because every bound above assumes that stack. Under the default
profile the work runs on the caller's thread, as before. *(The
security-round-2 implementation falls back inline under both
profiles; the docs round closes this.)*

The capability mirror is not a new rule. A script binds only the
`--mirror` it is given (§7, Step 7 of the host tutorial), and the
JIT refuses a foreign symbol the lowering did not import. A host
builds one mirror per trust level with `subscript bind` and passes
the narrow one to untrusted content. 109.7 adds the example.

Deferred, each for its own evidence: a checker work limit; a Worker
count limit as an alternative to S025.

### 109.3 What the lowering emits

A new `IntrinsicFamily::Sandbox` with two operations. The lowering
emits them only under the profile.

| Intrinsic | Placed | Effect |
|---|---|---|
| `Sandbox.Enter` | first instruction of every function body, after the parameter binds; the module initializer is a function body | calls `subscript_rt_sandbox_enter(ctx)`; then the pending-trap check |
| `Sandbox.Poll` | on the iteration edge of every loop the lowering emits, before the condition: the HIR loops (`while`, `for`, `for-of`) and the fused loops the lowering builds for a static callback, a static array callback, and `forEach` | calls `subscript_rt_sandbox_poll(ctx)`; then the pending-trap check |

*(Amended 2026-09-17, after the Phase Review.)* The first text named
`do`, which HIR does not have, and named the HIR loops only. The
module initializer got no `Enter`, and a fused callback loop got no
`Poll`, so the interval between checkpoints on such a loop was the
array length. Both are closed.

1. Both are calls into the shared runtime followed by the pending
   trap check that every runtime call already gets. No tier decides
   what they mean; the runtime does (§68.7).
2. The dev JIT, the C emitter, and the reference interpreter each
   lower the two intrinsics as that call. The gate stays total: a
   profile entry runs on every tier and matches its golden.
3. A host callback that re-enters a script function runs `Enter` at
   that function's entry, so the limits cover it.
4. `async` functions and coroutines are state machines on one native
   stack (§26). `Enter` runs at every resume that enters a function
   body, and `Poll` runs on every loop edge inside one. No second
   stack exists, so 109.4 rule 3 covers them.

### 109.4 What the runtime holds

Three limits on the Context. Each defaults to "none", so a trusted
program is unchanged. Each is one C API call.

| Limit | C API | Trap |
|---|---|---|
| interrupt | `subscript_rt_ctx_interrupt_handle(ctx)` on the owner thread; `subscript_rt_interrupt_set(handle)` from any thread | `Interrupted` = 25 |
| allocation quota | `subscript_rt_ctx_set_alloc_quota(ctx, bytes)`; 0 is none | `AllocationQuota` = 26 |
| stack budget | `subscript_rt_ctx_set_stack_budget(ctx, bytes)`; 0 is none | `StackBudget` = 27 |

1. **Interrupt.** The flag lives in its own heap cell, outside the
   Context's bytes, so a store from another thread never touches
   memory the owner thread holds exclusively. The host obtains the
   cell on the owner thread, before or between runs:
   `subscript_rt_ctx_interrupt_handle(ctx)` returns
   `const subscript_rt_interrupt*`, valid until the Context is
   released. `subscript_rt_interrupt_set(handle)` is callable from any
   thread and stores `true` with a relaxed store. `Enter` and `Poll`
   read the cell with a relaxed load. If the flag is set, the runtime
   records the `Interrupted` trap at the intrinsic's position and the
   trap-stop path returns to the host. `subscript_rt_ctx_clear_trap`
   clears the flag with the trap. *(Amended 2026-09-17, after the
   Phase Review: the first text stored through the Context pointer
   from the second thread, inside the owner's exclusive borrow.)*
2. **Allocation quota.** The §21 allocation path compares
   `live_bytes` plus the request against the quota before it
   allocates. Over the quota, it takes the §21 fault path with
   `AllocationQuota` in place of `AllocationFailure`. Strings, arrays,
   maps, sets, and objects all pass through that path, so no separate
   size limit exists. The runtime keeps `live_bytes` as a counter it
   maintains at every allocation, release, retention, and collection,
   in both memory modes. The quota check reads the counter and costs
   the same at every live count. A debug assertion compares the
   counter against the fold over the live set at every collection,
   and a test compares the two after each kind of change. *(Amended
   2026-09-16, after round 2: the first implementation folded over
   the live set at every allocation, measured quadratic at 10,000 and
   20,000 live allocations.)*

   **No buffer sized by script input exists outside the quota.** A
   runtime operation whose result size follows from its inputs
   computes that size first and allocates it through the Context
   (`alloc_str_with`, or the array and map paths), which checks the
   quota before any byte exists. Where the size is not known before
   the bytes are built, the buffer grows inside the quota's headroom
   (`Context::quota_headroom`, the bytes one more allocation can
   take): a temporary bounded by that headroom holds nothing past it,
   and the operation then reports the total it wanted to
   `check_quota`, which records the same trap as the final allocation. The temporary is not itself a Context allocation, so
   `live_bytes` and the trap position do not change. The headroom is
   in reserved bytes: the largest payload that fits is the headroom
   less the header and the record in the exact-size mode, and the
   largest size class at or under the headroom in the arena mode. A
   growing temporary holds at most twice its charge while its vector
   doubles. Two entries keep a bounded
   multiple instead: `toUpperCase` and `toLowerCase` build at most
   three bytes per receiver byte through the standard library's
   locale-free case mapping, and `JSON.parse` builds a transient
   document of about 40 bytes per node from an input the quota holds.
   Each charges its multiple against the headroom before it builds,
   so the bound of §109.0 holds for them too. A third multiple:
   `sort` holds two copies of its receiver while it sorts, and
   charges twice the receiver's bytes against the headroom before it
   begins. The `print` sink is Context memory that script output
   sizes: with no print observer installed, each `print` charges the
   line's bytes plus its newline to the quota before it appends, and
   `take_stdout` releases the charge. *(Amended 2026-09-17, fourth
   Phase Review: the first charge omitted the newline, so
   `print("")` grew the sink for free, 300 MB in 5 s.)* A callback binding record is charged at
   registration. Every bounded multiple is named here; an entry with
   a multiple this list does not name is a defect. A total check holds
   this: a test binary with a counting global allocator drives every
   `subscript_rt_*` entry whose result size a script controls, under a
   small quota and a huge request, and asserts the peak allocation
   stays under the quota plus a fixed slack; the same test derives
   the entry list from `ffi.rs` over **every** `subscript_rt_` export,
   whatever its family, and fails on an entry that is neither covered
   nor listed with a reason. *(Amended 2026-09-17, second Phase
   Review: the first check read six families of the sixteen, and
   `sort` was exempt with an unstated multiple.)* *(Amended 2026-09-17, after an external review:
   `String.repeat` built the result in a `Vec` before `alloc_str`,
   measured at 259 MiB resident for a 256 MiB request under a 64 MiB
   quota, before the trap.)*
3. **Stack budget.** `enter_script` at depth 0 records the address of
   a local as the floor. `subscript_rt_sandbox_enter` compares the
   address of its own local against the floor minus the budget. Below
   the floor minus the budget, it records `StackBudget`. The check
   runs after the frame of the entered function exists, so the
   overshoot past the budget is at most that one frame, which S027
   bounds at 65,536 bytes, plus the runtime's own call depth, which
   is under 65,536 bytes. The host sets a budget at least 131,072
   bytes below the thread's stack size. That is the host's fact,
   stated in the host tutorial. *(Amended 2026-09-17: the first text
   named no headroom.)*
4. A trap under the profile is an ordinary trap: first trap wins, the
   observer fires, the Context survives, the host reads it (§18.2).
5. The limits are the host's facts, not the program's. LIR carries
   the two intrinsics and no limit. Every runner, the CLI, and the
   interpreter harness set the limits the way a host does, through
   the C API or the runtime's own setters, before the entry runs.

### 109.5 Defaults under the CLI and the corpus harness

`subscript --profile sandbox` and a corpus entry with `profile:
sandbox` set the quota to 67,108,864 bytes and the stack budget to
524,288 bytes before the entry runs. A host embedding the runtime
sets its own. A Rust host, and every in-repo runner and benchmark,
sets its own through `RunConfig`: `alloc_quota` and `stack_budget`,
each `Option<u64>`, and a set value replaces the default. The
interrupt has no default; nothing sets it unless the host does. A
Rust caller of the in-process dev runner reaches the run's interrupt
cell through `RunConfig.interrupt_handle`, a sink the runner fills
before the first script call; the forked runner and the ship runner
refuse it, because their Context is in another process.
*(Amended 2026-09-17: round 3 did not measure `callbacks` because no
runner took a host quota.)*

### 109.6 Deferred: run-time source loading

"What you embed is the ship tier" (host tutorial). The runtime has no
API that compiles source at run time. A host that loads user content
at run time needs the dev tier behind a C ABI. That is a new surface
with its own dependency-size cost (measured at 3.08 MB for Cranelift
plus the C emitter together). It waits for a host that needs it, and
for its own section. Until then a host compiles user content with
`subscript build --profile sandbox` and links it as it links its own
scripts.

### 109.7 Corpus

| Entry | Kind | Proves |
|---|---|---|
| `r-sandbox-free` | reject | S023 |
| `r-sandbox-from-bytes` | reject | S024 |
| `r-sandbox-worker` | reject | S025 |
| `r-sandbox-source-depth` | reject | S026, depth 257 |
| `a-sandbox-clean` | accept | a profile program with a loop, a call, `bytesOf` on a scalar struct, and `Context.collect()` runs on every tier and matches its golden |
| `t-sandbox-alloc-quota` | trap | `AllocationQuota` at the allocation site, output before the trap intact. The entry reaches the quota with allocations of 4 KiB or more, so the two tiers' reserved sizes agree within 3% and the trap lands on the same allocation in both. A trap entry that reaches the quota with small objects is not constructible: the tiers charge them differently (§109.0). |
| `t-sandbox-stack-budget` | trap | `StackBudget` at the entry of the function the run failed to enter; the entry prints nothing that depends on the depth reached |

Each reject entry's header states what `tsc` does, measured. Each
reject entry also carries a twin without the header line, and that
twin is an accept entry: the same source under the default profile
runs. That twin is the firing control. The worker entry has no twin;
the existing worker accept entries are its control.

A profile rejection is not a TypeScript divergence: the surface is
unchanged and the default profile accepts the same source. §79 rule
1's divergence block does not apply to a reject entry whose header
selects a profile. The harness that pairs blocks with entries skips
such an entry and asserts that it skipped at least one.

The interrupt has no corpus entry, because the corpus has no second
thread. It has one test in `codegen/tests/` per tier: a profile
program with an infinite loop, a second thread that sets the flag
after 50 ms, and an assertion that the entry returns with
`Interrupted`. The same test without the flag set is the firing
control, bounded by a 2 s abort. The recorded number is the time from
the flag to the return.

### 109.8a Memory under the profile is reclaimed at a boundary

*(Added 2026-09-17.)* The profile rejects `Context.free`, so
collection is the one way memory returns. Nothing collects unbidden
(invariant 2), and the quota is a stop, not a pacer. Two patterns
keep a long-running profile program under its quota. Both are host
documentation; neither adds a rule to the compiler or the runtime.

1. **The host paces.** `subscript_rt_ctx_charged_bytes` reads the
   counter the quota compares against (§18.2d, §109.0), so a host
   reads it at every frame boundary at no cost. If the value is above
   the fraction of the quota the host chose, the host calls
   `subscript_rt_ctx_collect` there (§18.2d), outside any script
   call. The host, not the script, decides when. `live_bytes` is the
   payload figure and does not predict the trap. *(Amended
   2026-09-17, M6: the first text paced on `live_bytes`.)*
2. **The script collects at its own boundary.** `Context.collect()`
   stays callable under the profile. A script that calls it at the
   end of its frame function keeps its own live set bounded, and the
   host's pacer is the backstop.

The host tutorial shows both with a measured run, and
`examples/sandbox/` carries the program. A collect is stop-the-world
mark-sweep, proportional to the live set plus the dead set; a host
that needs a bound on its length has no rule here yet, and asks for
one with a measurement.

### 109.6a The heavy tests

*(Added 2026-09-17, round 9; measured in round 9b. Amended
2026-09-19: the memory-budget test left the heavy set.)* Two CLI tests
drive the compile child to its budgets. The time-budget
test's firing control runs under the 300 s budget (67.6 s
unoptimized on the owner's host). The process-group test builds a
host C file whose compile is the larger part of the build, and then
kills that build. Those heavy parts run only when
`SUBSCRIPT_HEAVY_TESTS=1` is set. Without it each test prints one
`gate-skip:` line that the gate counts (§85). The quick shape's
expected skip count is therefore 4 and the full shape's is 2. Both
run in the debug profile alone, and the release run declares each one
with a `gate-debug-only:` line (§85 rule 4a): the time budget is
300 s in each build, and a process-group kill is one code path in
each. The full shape's expected `gate-debug-only:` count is
therefore 2. The
time-budget test's 2 s stop runs in every shape. The full gate
exports the variable for the whole shape; the quick gate does not.
The gate record names the variable (§85 rule 5). Measured before the
memory-budget test left the set: the memory-budget and time-budget
tests take 2.7 s without the variable and 214.6 s with it; the debug
suite 358 s against 495 s.

**The memory-budget test is not heavy, because its budget is
replaced.** *(Added 2026-09-19.)* The test drove a real 12 GiB
compile, and the source that reached it was a chain the parser
handled in quadratic memory (§109.2 rule 6). The fork made that path
linear, so the source now completes in 1.65 s and the test's
assertion failed. A test whose subject is a defect of a dependency
holds only while the defect does.

The test now replaces the memory budget by the variable §109.2 rule 6
names, at a heap under what its own source demands, and its firing
control is the same source under a replaced heap with headroom. It
runs in every shape and in both profiles, and it checks what it always
checked: a compile over the budget is one S026 at the entry file, the
child's own record of its end passes through, and the parent
classifies that end. The contract's own heap is held by §109.2a's
derivation, not by a test that waits for a host able to host one.

*(Amended 2026-09-19, after the Windows host ran the test: the heap a
test states composes one limit for each host, and the child's own
record is the host's. Linux fails a heap allocation. Windows fails the
stack commit of the parser's recursion, and the record is the
overflow, not an allocation. §109.2 rule 6 names both ends. Evidence:
`specs/tracking/windows-portability.md`.)*

**Rule: a firing control does not run under the contract's default
budget.** The control measures the host, and the default budget is
one host's number. A control sets `SUBSCRIPT_COMPILE_TIME_BUDGET_SECONDS`
to the ceiling; the run under test keeps the budget the test derives
from the control's own wall time.

*(Amended 2026-09-18, after the x86-64 Linux gate host ran the suite.
The process-group test joined the heavy set, and the rule above is
new. That test's control held the contract's 300 s default, and it
passed the default on that host: the C compile of its 65,536-step
fixture alone is 686.39 s, and the whole control build is 632.92 s.
The test then reported S026 where it required the control's exit 0.
The 67.6 s above is the owner host's figure for a different test, and
the same class of assumption produced this defect. Owner decision
2026-09-18: the test stays heavy; the fixture is not made smaller.)*

**Rule: a heavy test does the expensive work once for each fact it
proves.** *(Added 2026-09-19; CLAUDE.md core principle 15.)* The
process-group test did it three times: a control, a killed build at
half the control, and a `sleep` of the control's own wall time. The
third pass broke §102 rule 3. The wait now ends on the end of the
child's process group, which is a fact of the host, and the killed
build's budget is a tenth of the control, not a half. The budget's
fraction must satisfy two conditions only: the emission finishes
inside the budget, which the test reads from the emitted `program.c`;
and the C compile has started, which the fixture makes true for every
small fraction.

Measured on `x86_64-unknown-linux-gnu`, unoptimized: the control
666.60 s, the killed build 66.01 s on its derived 66-second budget,
the wait for the group 50.14 ms, the whole test 733 s against
1,581.86 s before. The wait is the proof that it tracks a fact: with
the kill narrowed to the direct child, the same wait held 547 s and
the test then failed on the executable the surviving compiler wrote.
The Windows arm reads the child's own exit, because the Job Object
carries `KILL_ON_JOB_CLOSE` and the child holds the only handle. The
Windows host ran the heavy set on 2026-09-19 and both heavy tests
passed, the whole CLI suite in 82.39 s. Evidence:
`specs/tracking/linux-portability.md`,
`specs/tracking/windows-portability.md`.

**The group wait ends on `ESRCH` alone, and `EPERM` is a held
group.** *(Added 2026-09-19.)* `kill` with signal 0 on a process group
has three answers on macOS, measured on arm64. The answer is 0 while
a member lives. It is `EPERM` while every member is dead and a parent
did not collect one of them yet. It is `ESRCH` after that. The killed
compile child and its C compiler stay in that second state for a short
time, so the wait polls through `EPERM` as it polls through 0. Every
other error is a failure of the test. The arm carries no `cfg`: the
rule is the same on each Unix host.

### 109.7a The in-repo runners under the profile

*(Added 2026-09-17, round 7.)* `subscript run --profile sandbox`, the
dev-JIT runner, and the ship runner are hosts of this repository.
Under the profile they install no print observer: the Context sink,
charged to the quota (§109.4 rule 2), is their capture, and they read
it as they read the observer's buffer today. A host outside this
repository that installs an observer owns that buffer and its bound.

### 109.8 Exit criteria

1. Every 109.7 entry is Red at this contract's pin and Green after.
2. `tools/gate.sh full` is green.
3. The interrupt latency, per tier, is recorded in the tracking note.
4. The adversarial list runs and each item rejects or traps: a
   131,073-byte source; a 257-deep parenthesis source (the nesting
   guard, after the parse); recursion with no base case; an
   allocation loop; `fromBytes` of forged bytes; a regex literal that
   holds a quote before a deep nest. The outcome of each is recorded.
5. The benchmark matrix runs under the profile on the dev JIT and the
   ship tier. The ratio to the default profile is recorded per
   workload. No threshold; the number is the baseline.
6. `README.md` and `docs/tutorial-c-cpp.md` state the profile where
   they state "not a sandbox" today.
7. `tools/hygiene.sh` is clean at the Phase Review.
