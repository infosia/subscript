<!-- §210 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 210. A store to a shadow root reaches the MSVC output

*(Added 2026-10-30.)* Origin: `tools/gate.sh full` at `c7bb7e55` failed
on Windows. On 2026-10-30 the owner asked for the fix. The measurement
is in `specs/tracking/msvc-shadow-root-store.md`.

Problem, measured at the pin `c7bb7e55` with MSVC 19.44.35222: the ship
binary of `corpus/accept/a383-exhaustive-enum-switch` exits with
`0xc0000005`. The dev JIT and the interpreter print the golden. The
method `Tile.area` stores its receiver in `roots.v0`, then reads the
field through that slot. The `/O2` output stores 0 in the slot and
reads through a null pointer. The argument `a0` is not used.

| Compile | Result |
|---|---|
| `/Od` | prints `9` |
| `/O2 /Ob0` | `0xc0000005` |
| `/O2 /d2SSAOptimizer-` | prints `9` |
| `/O2 /Og-` | prints `9` |

The defect does not need the runtime. The emitted function, compiled
with three empty external stubs in a second file, gives the same
failure. A review of the emitted function found no undefined behaviour.
The result is a defect of `cl`. A soundness failure of the ship tier
results, on the toolchain that §11c selects.

### 210.1 What the fix must cover

1. **The emitted C carries the fix.** A host can compile the output of
   `subscript emit` with its own flags (`README.md`,
   `docs/tutorial-c-cpp.md`). A flag in `add_c11_optimized_flags`
   does not reach that host, so a flag alone does not close the defect.
2. **The fix covers the trigger, not `a383`.** The tracking note states
   the reduced trigger. The form must make that trigger unreachable for
   every function that the emitter writes.
3. **Other compilers see the same C.** The change applies only when
   `_MSC_VER` is defined and `__clang__` is not. For every other
   compiler, the preprocessed C does not change.
4. **The runtime contract does not change.** The frame keeps its
   layout and its word count, and `subscript_rt_shadow_push` keeps its
   signature.

### 210.2 The form

The tracking note holds the reduced trigger and the measured cost of
three forms that fail §210.4 rule 3: a `volatile` frame,
`/d2SSAOptimizer-`, and a `volatile` clear. None of them lands.

1. **The value of an emitted `switch` is a `volatile` local on MSVC.**
   The local `_disc` is `volatile`. The clears do not change.
2. **Every emitted `switch` uses this form.** One emitter site writes
   the local, so that a new `switch` cannot omit the form.
3. **The form closes the shapes of the tracking note.** The defect
   needs a `switch` in each shape that fails. If a reduction shows the
   defect without a `switch`, the landing stops and reports.

**Open.** The form does not prove that the class is closed. The cause
inside `cl` is not known, and a small number of shapes were run. A
shape with no `switch` that shows the defect is a new finding. The
differential gate reports such a shape only if a corpus entry has it.

### 210.3 Corpus

No new entry. `a383-exhaustive-enum-switch` is Red at the pin on MSVC.
Its golden does not move. A unit test reads the emitted text for the
`switch` and the MSVC condition. Any moved corpus golden is a stop.

### 210.4 Exit criteria

1. `a383` fails at `c7bb7e55` on MSVC and passes after the landing on
   the dev JIT, the ship C, the interpreter, and the golden, in the
   debug and the release sweeps.
2. The standalone reproducer in the tracking note fails at `/O2` with
   the emitted function of the pin. It prints `9` with the emitted
   function of the landing.
3. Cost: the ship column of the cross-language benchmark on MSVC, at
   the pin and after, for each workload. If the median of a workload
   is more than 5% slower, the landing stops and reports the numbers.
   The tracking note records each pair.
4. `tools/gate.sh full` is green.
5. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
