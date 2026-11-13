<!-- §275 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 275. A Worker Context spawns no Worker

*(Added 2026-11-13.)* Origin: the full gate of 2026-11-13 at `8638c8d0`
on Windows did not end. The owner selected the form on 2026-11-13: a
`Worker.spawn` that runs in a Worker Context traps.

Problem, measured at `8638c8d0` on Windows with the debug test binary
of `subscript-codegen`. The test
`reload::tests::failed_initializer_with_worker_reload_succeeds`
(`codegen/src/reload/tests.rs`, §272) did not end in 15 seconds when it
ran alone. In the gate, the binary held about 486,000 threads and
27.6 GB of private memory after 31 minutes. The machine needed a
restart.

The cause, read in the code. A Worker thread makes its own Context and
runs the module initializer in it (`run_worker`,
`runtime/src/worker.rs`; §39). The program of the test calls
`Worker.spawn` from the module initializer. So each Worker runs the
initializer, and the initializer spawns the next Worker. A Context
joins each Worker when it drops (`WorkerSet::shutdown`), so no thread
of the chain ends. The chain stops only when thread creation fails.

A program in good faith reaches this: a module global whose
initializer calls a function that spawns a Worker. A Worker entry that
calls `Worker.spawn` is the other form of the same class. The fix
closes both forms with one rule.

### 275.1 Rules

1. A Context records whether it is a Worker Context.
   `Context::new_worker` sets the record; no other constructor sets it.
2. `Worker.spawn` in a Worker Context starts no thread. It traps with
   a new trap kind, `worker-spawn-in-worker`. The kind takes the next
   free value of `TrapKind` (`runtime/src/trap.rs`). The trap message
   states that a Worker cannot spawn a Worker.
3. Rule 2 is a check in `Context::worker_spawn`. Each tier reaches
   that function, so the development tier and the ship tier give the
   same trap.
4. The trap of rule 2 carries the position id of the `Worker.spawn`
   call. The spawn entry point (`subscript_rt_worker_spawn`) takes that
   position id as a parameter. Each tier passes the position id of the
   call, as it does for each other trap site.
5. The trap follows the present Worker trap path. If the Worker
   initializer traps, the entry does not run (`run_worker`). The parent
   observes `worker-trapped` with the site of the `Worker.spawn` call in
   the Worker, by §152 rule 1.
6. A main Context keeps the present behaviour of `Worker.spawn`.
7. This section replaces the rule "Workers may spawn workers" of §39.1
   and of `specs/blocks/stdlib.md` §16.1.
8. A consequence of rules 2 and 5: if a module initializer calls
   `Worker.spawn`, each Worker of that program traps in its initializer.

### 275.2 Acceptance

1. Red first, at `8638c8d0`: a new corpus trap entry `t114` has a
   Worker entry that calls `Worker.spawn`. At the pin, both tiers run
   it to the end with no trap. The tracking note records the output.
   Each run of the Red step has a hard time limit of 30 seconds.
2. After the change, `t114` traps `worker-trapped` in both tiers, at
   the site of the `Worker.spawn` call in the Worker entry. Its golden
   pins the output.
3. A runtime unit test makes a Worker Context, calls `worker_spawn`,
   and reads the trap kind `worker-spawn-in-worker`. A control calls
   `worker_spawn` in a main Context and reads no trap.
4. `reload::tests::failed_initializer_with_worker_reload_succeeds`
   ends. Its wall time on Windows is under 2 seconds, measured.
5. The `TrapKind` value tests in `runtime/src/trap.rs` cover the new
   kind: the value, `from_u32`, and the rule name.
6. No other corpus entry or golden changes, except the trap entry
   `t115` of item 8.
8. A trap entry `t115` pins rule 8. A module global initializer calls
   a function that spawns a Worker, joins it, and returns. The Worker
   runs the same initializer, and its spawn traps. In both tiers the
   main initializer then traps `worker-trapped` at the join, with the
   site of the `Worker.spawn` call in that function.
7. The full gate ends on Windows. The tracking note records the
   verdict line.

### 275.3 Phase Review

Pending.

### 275.4 Open

None.
