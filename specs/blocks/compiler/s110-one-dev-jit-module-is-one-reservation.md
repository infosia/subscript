<!-- §110 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 110. One dev-JIT module is one reservation

*(Owner decision 2026-09-18.)* Origin: the x86-64 Linux gate host.
Evidence: `specs/tracking/linux-portability.md`, the defect-3 sections.

Problem: the dev JIT built each module's code and its data from
separate mappings, and it assumed without stating it that every item
stays within 2 GiB of every other. `JITModule::new` refuses
`is_pic`, so a call the module makes to its own data carries a 32-bit
displacement. §109.2a (history) then gave the compile thread a stack
reservation of 8,589,934,592 bytes unoptimized. The lowering runs on
that thread, the two mappings fell on the two sides of the
reservation, and the JIT failed to connect them.

Measured on `x86_64-unknown-linux-gnu`: the pair was
`subscript_export_main` to `subscript_str0`, and the displacement was
−8,567,550,719 in one run and −8,573,338,367 in another. Neither end
was a host symbol.

### 110.1 The rule

1. **A dev-JIT module holds its code and its data in one
   reservation.** `JITBuilder::memory_provider` installs a
   `cranelift_jit::ArenaMemoryProvider`, which reserves one contiguous
   region and carves its code, read-write, and read-only segments from
   it. Every site that builds a dev-JIT module installs one: the JIT
   runner and the hot-reload session. A module built without one is a
   defect of this section.
2. **The displacement between two items of one module is bounded by
   that module's reservation, and by nothing else.** No stack size, no
   allocation order, and no address-space layout takes part. A test
   pins the bound, and it reads the two addresses from the finished
   module, not from the expression that placed them.
3. **The reservation's size is derived from the module's own source
   bytes.** The form carries the fact: a module knows the bytes it was
   compiled from. The size is

       reserve = floor + slope × source bytes, times the margin 1.5

   `floor` and `slope` are measured, never chosen. `slope` is the
   worst bytes of reservation per source byte over the corpus, and
   `floor` is the worst reservation of a module whose source is too
   small for the slope to reach — the per-module cost that does not
   scale with the source. A round that measures a worse figure moves
   the constant and records the measurement. The margin is 1.5.
4. **A reservation that runs out names what to change.** The message
   states the module's source bytes, the reservation the derivation
   gave, and the bytes the module asked for. It is an internal error,
   not a diagnostic: the program is not at fault, the constant of rule
   3 is. The message is the report that moves the constant.
5. **The dependency aborts where this project returns an error, and
   that is a recorded divergence.** `Segment::set_rw` calls `.expect`
   (`cranelift-jit-0.125.4/src/memory/arena.rs:44`), so an `mprotect`
   failure ends the process. `SystemMemoryProvider` returns an error
   at the same point. Core principle 5 forbids the panic, and the
   panic is in the dependency.

   The threshold moves the other way, so the divergence costs less
   than it looks. Measured against `vm.max_map_count` 65,530: the
   arena takes 3 mappings for each reload generation and the earlier
   provider took 4, and the first generation to fail is 21,808 against
   16,361. Resident memory per generation is about 140 kB in each. A
   round that needs the error instead of the abort forks the
   dependency and pins the fork; this section does not order that
   fork.

### 110.2 What this does not change

The reservation is `mmap` with no access, so it costs address space
and not resident memory. Measured: peak resident memory of the
heaviest dev-JIT subject is 390,344 kB with the arena against
390,152–395,232 kB without. Wall time is inside this host's own
spread; a control with the reservation cut to 4 MiB measured the same
wall time as the full one, so the size of the reservation costs
nothing.

Hot reload keeps its shape. Each generation builds its own module and
its own reservation, and the session holds every generation until it
drops, because a function pointer of a retired generation stays valid.

### 110.3 Exit criteria

1. `boundary_scratch_breadth` passes five runs of five on the x86-64
   Linux host, unoptimized. The defect is not deterministic, so one
   run proves nothing.
2. The test of rule 2 fails against a binary built without the
   reservation.
3. `floor` and `slope` are measured over every dev-JIT module the gate
   builds, and the measurement is in
   `specs/tracking/linux-portability.md` with the module count.
4. `tools/gate.sh full` is green on the x86-64 Linux host.
5. No committed golden, `.expected` file, corpus entry, or
   `benchmarks/results.json` moves.
