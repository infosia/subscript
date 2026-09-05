# §86 — C emission is linear in the function it emits

Status: **in progress.** Contract: `specs/blocks/compiler.md` §86
(Rev 1 at `f683a72`). Origin:
`specs/tracking/development-cost-review-2026-09-05.md` finding 2.

## Round 1 and 1b — the profile (no code change), at `97c9991`

Measured by the coding agent with temporary timers, release
profile, Apple M2, one run per width. The section below is the
agent's report, unchanged except for this heading.

---


Date: 2026-09-05. Revision: `97c99919acfc10142f4ffbf27a1d47bb707df29b`.
Host: Apple M2, 8 CPU cores, 16 GB memory, macOS 26.6.2 (25G83), arm64.
Compiler: rustc 1.95.0 (59807616e 2026-04-14), aarch64-apple-darwin. Cargo profile: release, optimized.

## Stage table

All times are wall-clock seconds from `std::time::Instant`.
Each width has one measured execution in a separate process. No statistical aggregation applies.
The probe copies `mirror`, `aggregate_new`, `program`, and `checked` from `codegen/tests/boundary_scratch_breadth.rs`.
It replaces `MAX_POSITIONS` with the requested width and calls `checked(false)`.
Each program covers counts 1 through its width, with empty sibling payloads, one function, and one block.
The instruction counts match §86's fixture counts.

| Width | HIR→LIR | LIR verification | Root plan | Coalescing: value_interference | Coalescing: merge | Coalescing: other | Coalescing total¹ | Text emission² | Other emit overhead | emit_c total | LIR instructions | Values | Address-taken values | Root slots | Maximum RSS, bytes³ | Maximum RSS, GiB³ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 0.004995958 | 0.003541750 | 0.027694291 | 0.020768250 | 0.000160500 | 0.000082916 | 0.021011666 | 0.030504709 | 0.000991709 | 0.088740083 | 2400 | 1576 | 0 | 157 | 16908288 | 0.015747 |
| 16 | 0.004858250 | 0.006459708 | 0.253301750 | 0.316575625 | 0.001276333 | 0.002065084 | 0.319917042 | 0.389427333 | 0.001253667 | 0.975217750 | 11072 | 6928 | 0 | 505 | 112885760 | 0.105133 |
| 32 | 0.022915459 | 0.047118958 | 6.442321000 | 12.185086292 | 0.014835042 | 0.012916416 | 12.212837750 | 8.488802208 | 0.005961417 | 27.219956792 | 59520 | 35104 | 0 | 1777 | 1712291840 | 1.594696 |

1. Coalescing total includes `value_interference`, merge work, graph destruction, and timer output. Do not add this subtotal again.
   The merge interval includes `Coalescing::new`, both merge loops, and `representatives`.
   Coalescing other is the call total minus its interference and merge intervals.
2. Text emission spans root-plan return through both final C-text checks, minus the complete coalescing call.
   It includes all other work after the storage plan, including declaration analysis and C output.
   The inclusive post-plan times are 0.051516375, 0.709344375, and 20.701639958 seconds for widths 8, 16, and 32.
   Other emit overhead is the total minus the exclusive stages. It includes work before the root plan and LIR destruction.
3. These RSS figures come from `getrusage(RUSAGE_SELF).ru_maxrss`, in bytes on this Darwin host.
   The probe samples after it drops the emitted C and HIR. RSS covers the whole process, including fixture construction and type checking.
   `/usr/bin/time -l` failed before its RSS output at every width:

   ```text
   time: sysctl kern.clockrate: Operation not permitted
   ```

   Its exit status was 1. The probe completed and emitted all measurements before that error.
   Thus, the requested `/usr/bin/time -l` RSS figures are unavailable under this sandbox; the table explicitly uses the fallback.
   The command reported process real/user/system seconds of 0.53/0.09/0.00, 0.98/0.97/0.01, and 27.26/27.09/0.15.

`lower_module` verifies its result. `emit_lir_c` verifies the module again.
The verification column adds both intervals; HIR→LIR subtracts the first verification interval from the `lower_module` call.
The first verification intervals are 0.001869417, 0.003349583, and 0.024006875 seconds.
The second intervals are 0.001672333, 0.003110125, and 0.023112083 seconds.
Fixture construction and `check_program` are outside `emit_c` time. Build time is outside all measurements.
Timer output adds some overhead. Nine decimal places preserve the raw measurements; they do not imply equivalent accuracy.

## Dominant stage and site

At width 32, `coalesced_value_storage` takes 12.212837750 seconds.
Its `root_storage::value_interference` call dominates the stage table at **12.185086292 seconds**, about 44.8% of `emit_c`.
The call site is `codegen/src/cemit.rs:2115`.

In that stage, `codegen/src/root_storage.rs:207` visits instructions; line 210 visits the live set for each instruction result.
Line 211 inserts interference edges into the symmetric `Vec<HashSet<ValueId>>` graph.
This work scales with the product of instruction results and live-set size.

A second product occurs at `codegen/src/root_storage.rs:177`: each origin edge expands through both origin-member groups.
Lines 177 and 178 visit every left-member/right-member pair; line 179 inserts the corresponding edges.
This work scales with the product of the two group sizes per origin edge.
The stage timer includes both graph operations; it does not isolate their individual costs.

`address_taken_values` contains **zero values at every width**.
The measured graph cost therefore does not establish that held-to-exit values cause this fixture's cost at this revision.
These line numbers refer to the restored production files. No fix forms part of this round.

## Exact measurement commands

Commands run from the repository root follow. The appendix contains the exact temporary setup scripts.
The first build preceded discovery of the verification inside `lower_module`; no fixture ran with that first binary.

```sh
mkdir -p /tmp/subscript-s86-round1
python3 /tmp/subscript-s86-round1/instrument.py
cargo build --release -p subscript-codegen --example s86_measure > /tmp/subscript-s86-round1/build.log 2>&1
python3 /tmp/subscript-s86-round1/instrument-lowering.py
cargo build --release -p subscript-codegen --example s86_measure > /tmp/subscript-s86-round1/build-final.log 2>&1
```

This first measurement loop stopped at width 8 because `/usr/bin/time -l` returned 1.
Its width-8 total was 0.095486917 seconds. The table excludes this preliminary execution.

```sh
for width in 8 16 32; do /usr/bin/time -l target/release/examples/s86_measure "$width" > "/tmp/subscript-s86-round1/width-$width.log" 2>&1; result=$?; cat "/tmp/subscript-s86-round1/width-$width.log"; if [ "$result" -ne 0 ]; then exit "$result"; fi; done
python3 /tmp/subscript-s86-round1/instrument-rss.py
cargo build --release -p subscript-codegen --example s86_measure > /tmp/subscript-s86-round1/build-rss.log 2>&1
for width in 8 16 32; do /usr/bin/time -l target/release/examples/s86_measure "$width" > "/tmp/subscript-s86-round1/final-width-$width.log" 2>&1; result=$?; cat "/tmp/subscript-s86-round1/final-width-$width.log"; echo "time_exit_status=$result"; done
```

Host and revision commands:

```sh
uname -sm
sw_vers
sysctl -n hw.model hw.memsize hw.ncpu
rustc -Vv
git rev-parse HEAD
/usr/sbin/system_profiler SPHardwareDataType | head -18
```

The `sysctl` hardware query failed with `Operation not permitted`.
`system_profiler` supplied the hardware facts above despite a CPU-family query warning.

Cleanup commands:

```sh
python3 - <<'PY_CLEANUP'
from pathlib import Path
import shutil
base = Path('/tmp/subscript-s86-round1')
shutil.copyfile('codegen/examples/s86_measure.rs', base / 's86_measure.rs')
for name in ['lib.rs', 'cemit.rs', 'root_storage.rs', 'lir.rs']:
    shutil.copyfile(base / name, Path('codegen/src') / name)
Path('codegen/examples/s86_measure.rs').unlink()
PY_CLEANUP
git diff --stat
git status --short
```

Both Git commands produced empty output after cleanup, before this report overwrite.
Final checks after the report overwrite:

```sh
git diff --check
git diff --stat -- . ':!REPORT.md'
git status --short
```

The requested statement that ordinary `git status` shows only `REPORT.md` is not true in this checkout.
`REPORT.md` is untracked and ignored by `.gitignore:2`; ordinary `git status --short` produces empty output.
The attempt to expose the report without a commit failed:

```sh
git ls-files -v REPORT.md HANDOFF.md
git check-ignore -v REPORT.md
git add --intent-to-add --force REPORT.md
```

```text
.gitignore:2:REPORT.md REPORT.md
fatal: Unable to create the repository's .git/index.lock: Operation not permitted
```

The final error above abbreviates the host-specific repository path.
The sandbox prevents the index change. The ignore rules remain unchanged.
This explicit ignored-file status command shows only `REPORT.md`:

```sh
git status --short --ignored -- REPORT.md
```

```text
!! REPORT.md
```

Thus, only the report content differs; ordinary status visibility remains blocked by the existing ignore rule and sandbox.

All temporary source timers and the repository probe are removed. Production files match their initial bytes.
No file under `specs/` changed. No commit or `tools/gate.sh` execution occurred.

## Temporary setup scripts

These scripts document the measurements. Their source changes are absent from the final tree.

### instrument.py

```python
from pathlib import Path
import shutil
base = Path('/tmp/subscript-s86-round1')
for name in ['lib.rs', 'cemit.rs', 'root_storage.rs']:
    shutil.copyfile(Path('codegen/src') / name, base / name)

def edit(name, old, new):
    p = Path('codegen/src') / name
    s = p.read_text()
    assert s.count(old) == 1, (name, old, s.count(old))
    p.write_text(s.replace(old, new))

fixture = Path('codegen/tests/boundary_scratch_breadth.rs').read_text()
probe = 'use std::fmt::Write as _;\nuse subscript_codegen::emit_c;\nuse subscript_compiler::{check_program, SourceFile};\n'
probe += fixture[fixture.index('fn mirror()'):fixture.index('fn address_plan(')]
probe = probe.replace('MAX_POSITIONS', 'width()')
probe += '''
fn width() -> usize {
    std::env::args().nth(1).unwrap().parse().unwrap()
}
fn main() {
    assert!([8, 16, 32].contains(&width()));
    let hir = checked(false);
    let start = std::time::Instant::now();
    let c = emit_c(&hir).expect("breadth fixture emits C");
    let total = start.elapsed().as_secs_f64();
    eprintln!("S86 width={} emit_c_total={total:.9} source_bytes={}", width(), c.source.len());
    std::hint::black_box(&c);
}
'''
Path('codegen/examples').mkdir(exist_ok=True)
Path('codegen/examples/s86_measure.rs').write_text(probe)
edit('lib.rs', '''    cemit::emit_lir_c(&lir, true)''', '''    eprintln!("S86 lowering={:.9}", s86_lower.elapsed().as_secs_f64());
    assert_eq!(lir.functions.len(), 1);
    for f in &lir.functions {
        eprintln!("S86 instructions={} values={} blocks={}", f.blocks.iter().map(|b| b.instructions.len()).sum::<usize>(), f.values.len(), f.blocks.len());
    }
    cemit::emit_lir_c(&lir, true)''')
edit('lib.rs', '''pub fn emit_c(module: &subscript_compiler::hir::Module) -> Result<CProgram, String> {
    reject_discovery_hir_for_c(module)?;''', '''pub fn emit_c(module: &subscript_compiler::hir::Module) -> Result<CProgram, String> {
    reject_discovery_hir_for_c(module)?;
    let s86_lower = std::time::Instant::now();''')
edit('cemit.rs', 'use std::fmt::Write as _;', '''use std::fmt::Write as _;
thread_local! {
    static S86_AFTER_PLAN: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    static S86_COALESCE: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}''')
edit('cemit.rs', '''    verify_module(module).map_err(|errors| {''', '''    let s86_verify = std::time::Instant::now();
    verify_module(module).map_err(|errors| {''')
edit('cemit.rs', '''    let program = Emitter::new(module)?.emit(require_main)?;''', '''    eprintln!("S86 verification={:.9}", s86_verify.elapsed().as_secs_f64());
    let program = Emitter::new(module)?.emit(require_main)?;''')
edit('cemit.rs', '''    verify_no_label_before_declaration(&program)?;
    Ok(program)''', '''    verify_no_label_before_declaration(&program)?;
    let after_plan = S86_AFTER_PLAN.with(|v| v.get().unwrap().elapsed().as_secs_f64());
    eprintln!("S86 after_plan_inclusive={after_plan:.9} text_exclusive={:.9}", after_plan - S86_COALESCE.with(|v| v.get()));
    Ok(program)''')
edit('cemit.rs', '''    let interference = root_storage::value_interference(function)?;
    let mut coalescing''', '''    let s86_interference = std::time::Instant::now();
    let interference = root_storage::value_interference(function)?;
    eprintln!("S86 coalescing_interference={:.9}", s86_interference.elapsed().as_secs_f64());
    let s86_merge = std::time::Instant::now();
    let mut coalescing''')
edit('cemit.rs', '''    Ok(coalescing.representatives())''', '''    let representatives = coalescing.representatives();
    eprintln!("S86 coalescing_merge={:.9}", s86_merge.elapsed().as_secs_f64());
    Ok(representatives)''')
edit('cemit.rs', '''        let root_storage = root_storage::plan(function, &emitter.layouts)?;''', '''        let s86_plan = std::time::Instant::now();
        let root_storage = root_storage::plan(function, &emitter.layouts)?;
        let plan_seconds = s86_plan.elapsed().as_secs_f64();
        S86_AFTER_PLAN.with(|v| v.set(Some(std::time::Instant::now())));
        eprintln!("S86 root_plan={plan_seconds:.9} root_slots={}", root_storage.slots.len());''')
edit('cemit.rs', '''        let value_storage = coalesced_value_storage(''', '''        let s86_coalesce = std::time::Instant::now();
        let value_storage = coalesced_value_storage(''')
edit('cemit.rs', '''        let declaration_scopes = declaration_scopes(''', '''        let coalesce_seconds = s86_coalesce.elapsed().as_secs_f64();
        S86_COALESCE.with(|v| v.set(coalesce_seconds));
        eprintln!("S86 coalescing_total={coalesce_seconds:.9}");
        let declaration_scopes = declaration_scopes(''')
edit('root_storage.rs', '''pub(crate) fn plan(function: &l::Function, layouts: &Layouts) -> Result<RootStoragePlan, String> {
    let held_to_exit = address_taken_values(function)?;''', '''pub(crate) fn plan(function: &l::Function, layouts: &Layouts) -> Result<RootStoragePlan, String> {
    let held_to_exit = address_taken_values(function)?;
    eprintln!("S86 address_taken_values={}", held_to_exit.len());''')
```

### instrument-lowering.py

```python
from pathlib import Path
import shutil
p = Path('codegen/src/lir.rs')
shutil.copyfile(p, '/tmp/subscript-s86-round1/lir.rs')
s = p.read_text()
old = '    if let Err(errors) = verify_module(&lowered) {'
assert s.count(old) == 1
s = s.replace(old, '    let s86_lower_verify = std::time::Instant::now();\n' + old)
old = '    Ok(lowered)\n}'
assert s.count(old) == 1
s = s.replace(old, '    eprintln!("S86 lowering_verification={:.9}", s86_lower_verify.elapsed().as_secs_f64());\n' + old)
p.write_text(s)
```

### instrument-rss.py

```python
from pathlib import Path
p = Path('codegen/examples/s86_measure.rs')
s = p.read_text()
old = '    std::hint::black_box(&c);'
assert s.count(old) == 1
s = s.replace(old, '''    std::hint::black_box(&c);
    drop(c);
    drop(hir);
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the output on success.
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(status, 0);
    // SAFETY: the successful call initialized usage.
    let usage = unsafe { usage.assume_init() };
    eprintln!("S86 max_rss_bytes={}", usage.ru_maxrss);''')
p.write_text(s)
```


## Round 1b

Date: 2026-09-05. Same revision, host, release profile, fixture generator, and `checked(false)` payload as Round 1.
One new process ran at width 16; one new process ran at width 32. No repeat or averaging applies.
Round 1 content above remains byte-for-byte unchanged.

### Root-storage plan

All table times are seconds. Intervals use integer nanoseconds from `std::time::Instant`.

| Interval | Width 16 | Width 32 |
|---|---:|---:|
| `root_storage::plan` → `value_interference_with` | 0.198480000 | 6.267252042 |
| `root_storage::plan` slot-assignment loop | 0.003635125 | 0.101790916 |
| `root_storage::plan` tail | 0.189959250 | 4.668760125 |
| `root_storage::plan` remainder | 0.003281416 | 0.014391084 |
| **`root_storage::plan` total** | 0.395355791 | 11.052194167 |

- `value_interference_with` wraps only that call, including its maximum-live diagnostic.
- Slot assignment starts before `slots` and `origin_slots` initialization. It ends after the `for value in &function.values` loop.
- The loop includes `slots.iter().position(...)`. Its restored site is `codegen/src/root_storage.rs:290`; the scan starts at line 300.
- Tail starts at offset assignment, `codegen/src/root_storage.rs:323`. It includes `value_slots`, live sets, and both clear-set computations.
- Remainder includes initial `address_taken_values` and `live_ins`, edge-count diagnostics, result construction, local destruction, and timer bookkeeping.

The four disjoint intervals sum exactly to the plan total in integer nanoseconds.
At width 32, the slot-assignment loop takes 0.101790916 seconds; the graph call and tail take 6.267252042 and 4.668760125 seconds.

### Post-plan decomposition

The boundary matches Round 1: start immediately after `root_storage::plan` returns; end after `verify_no_label_before_declaration` returns.
Each function interval includes its callees unless the table explicitly excludes a separately measured call.
Standard-library operations remain inside their enclosing interval; they have no separate timers.

| Disjoint interval | Width 16 | Width 32 |
|---|---:|---:|
| `Body::new` pre-coalescing region | 0.151304750 | 5.002577042 |
| `coalesced_value_storage` | 0.550867583 | 17.327003959 |
| `declaration_scopes` | 0.000776375 | 0.004236292 |
| `declaration_can_use_instruction_assignment` (call-time sum) | 0.000123646 | 0.000682148 |
| `Body::new` delayed-declarations loop, exclusive | 0.057827063 | 1.753896227 |
| `Body::emit_storage` | 0.000234666 | 0.001337709 |
| `Body::emit_parameter_initializers` | 0.000000250 | 0.000000250 |
| `Body::emit_graph` | 0.447798000 | 7.193689916 |
| `Body::emit_unwind` | 0.000000542 | 0.000002542 |
| `runtime_traps` | 0.000142833 | 0.000678042 |
| `verify_trap_consumption` | 0.000676875 | 0.012235708 |
| `Emitter::emit_function` wrapper-text region | 0.000001917 | 0.000005708 |
| `Emitter::emit_init_and_exports` | 0.002487542 | 0.006128750 |
| `Emitter::emit_worker_adapters` | 0.000000167 | 0.000000042 |
| `render_allocation_metadata_definitions` | 0.000723333 | 0.001378000 |
| `render_allocation_metadata_header` | 0.000000375 | 0.000000375 |
| `verify_no_empty_aggregate` | 0.003872417 | 0.016761417 |
| `verify_no_label_before_declaration` | 0.004179625 | 0.011800375 |
| Post-plan remainder: assembly, destruction, bookkeeping | 0.000835333 | 0.006058998 |
| **Post-plan inclusive total** | 1.221853292 | 31.338473500 |

The disjoint rows sum exactly to **1,221,853,292 ns** at width 16 and **31,338,473,500 ns** at width 32.
The remainder is the enclosing interval minus all named disjoint intervals; it is positive at both widths.

After coalescing, `Body::new` directly calls two repository functions: `declaration_scopes` and `declaration_can_use_instruction_assignment`.
Both have separate timers. The predicate has 5,384 calls at width 16 and 29,200 calls at width 32.
Its table entry sums only those calls inside `Body::new`; calls inside other functions remain in those functions' intervals.
The delayed-declarations row subtracts this predicate-time sum from its enclosing loop time. It excludes `declaration_scopes` entirely.
The loop retains standard-library operations and per-call timer bookkeeping.
Its restored site is `codegen/src/cemit.rs:2468`; each value searches the block instructions at line 2471.
`declaration_scopes` starts at line 2167; the predicate definition starts at line 1778.

The pre-coalescing region includes all `Body::new` work between plan return and the coalescing call.
That region includes `fixed_iterator_values`, address analysis, local promotion, managed-local checks, and `removable_block_parameter_copies`.
It is outside the requested after-coalescing function split, but remains necessary for the post-plan sum.

The four `Body::emit_*` calls at `codegen/src/cemit.rs:1194` through line 1198 append the ordinary function body.
`Body::emit_graph` includes `emit_dominator_subtree` and its instruction, root-clear, and terminator output.
`runtime_traps` and `verify_trap_consumption` have separate, non-overlapping intervals at the following call site, line 1199.
The wrapper-text region includes `wrapper_signature`, argument formatting, and wrapper output after the ordinary function returns.
The other emitter and metadata functions cover their subsequent output.
Both final C-text verification passes have separate intervals.

The post-plan remainder includes the `Body` result construction and destruction, literal braces and goto output, and translation-unit assembly.
It also includes metadata attachment, emitter-local destruction, function-loop overhead, and timer bookkeeping outside the named intervals.
No enclosing `emit_function`, `emit_ordinary_function`, or `Emitter::emit` subtotal is added to the disjoint rows.

At width 32, `declaration_scopes` takes 0.004236292 seconds; the separate declaration-search loop takes 1.753896227 seconds.
The largest C body-output interval is `Body::emit_graph`, at 7.193689916 seconds.
The two final C-text checks total 0.028561792 seconds.

### Comparison with Round 1

| Same post-plan boundary | Width 16 | Width 32 |
|---|---:|---:|
| Round 1 | 0.709344375 | 20.701639958 |
| Round 1b | 1.221853292 | 31.338473500 |

These are new wall-clock observations. The new intervals sum to the new enclosing observation, not to the historical Round 1 observations.
Round 1 did not record these subintervals, so its exact historical decomposition is unavailable.
No time was rescaled to force agreement. Single runs do not establish the cause of the timing difference.
The added diagnostics and timers affect this run; the predicate timer especially limits conclusions about that small function's standalone cost.

### Graph counts, totals, and memory

Each edge count is `graph.iter().map(|set| set.len()).sum::<usize>()`, as requested.
The graph is symmetric: this counts both directions, rather than undirected pairs.
The counters inspect the existing graphs; they do not call an extra interference computation.

| Measurement | Width 16 | Width 32 |
|---|---:|---:|
| LIR instructions | 11,072 | 59,520 |
| Values | 6,928 | 35,104 |
| Address-taken values | 0 | 0 |
| Root slots | 505 | 1,777 |
| Plan `value_interference_with`: sum of set sizes | 3,178,680 | 62,810,096 |
| Coalescing `value_interference`: sum of set sizes | 3,178,680 | 62,810,096 |
| Maximum reverse-walk live-set size, both graph calls | 427 | 1,619 |
| Emitted C source bytes | 1,102,957 | 5,066,731 |
| Maximum RSS, bytes, `getrusage` | 113,606,656 | 1,714,225,152 |
| `emit_c` total, seconds | 1.637826542 | 42.483561959 |
| `lower_module`, including first verification, seconds | 0.014965417 | 0.054559667 |
| First LIR verification, seconds | 0.006612917 | 0.027763833 |
| Second LIR verification, seconds | 0.003656750 | 0.027094708 |

At width 32, each graph contains **62,810,096 set entries**. Both reverse walks reach **1,619 live values**.
The maximum samples the live set after terminator uses and after each reverse instruction update, including held values and recorded operands.
The update removes the result before it adds operands and invalidations; the sampled endpoints therefore cover the maximum live-set size.
This measures the actual `live` set in `value_interference_with`, not the plan tail's `occupied_during` union.

As in Round 1, `/usr/bin/time -l` completed the child, then failed at `sysctl kern.clockrate` with `Operation not permitted`.
It returned status 1 at both widths and supplied no RSS figure.
The fallback samples Darwin `getrusage(RUSAGE_SELF).ru_maxrss` after C and HIR destruction; the values are whole-process high-water marks in bytes.
The command reported real/user/system seconds of 2.17/1.56/0.02 at width 16 and 42.54/40.75/0.30 at width 32.

### Commands and restoration

Exact measurement commands, from the repository root:

```sh
mkdir -p /tmp/subscript-s86-round1b
python3 /tmp/subscript-s86-round1b/instrument.py
cargo build --release -p subscript-codegen --example s86_measure > /tmp/subscript-s86-round1b/build.log 2>&1
for width in 16 32; do /usr/bin/time -l target/release/examples/s86_measure "$width" > "/tmp/subscript-s86-round1b/width-$width.log" 2>&1; result=$?; cat "/tmp/subscript-s86-round1b/width-$width.log"; echo "time_exit_status=$result"; done
```

Exact source-restoration commands:

```sh
python3 - <<'PY_RESTORE'
from pathlib import Path
import shutil
base = Path('/tmp/subscript-s86-round1b')
for name in ['lib.rs', 'lir.rs', 'cemit.rs', 'root_storage.rs']:
    shutil.copyfile(base / name, Path('codegen/src') / name)
Path('codegen/examples/s86_measure.rs').unlink()
PY_RESTORE
git diff --check
git diff --stat
git status --short
```

All three Git checks produced empty output after source restoration.
Every production file matches its pre-round backup byte-for-byte; the repository probe is absent. **The tree is restored.**
Only `REPORT.md` content changed. Its Round 1 prefix matches the pre-round backup byte-for-byte.
No specs edit, commit, or `tools/gate.sh` execution occurred.
The existing `.gitignore` rule still hides `REPORT.md` from ordinary status. No index update was retried.
`git status --short --ignored -- REPORT.md` shows only `!! REPORT.md`.

Final report and restoration checks:

```sh
python3 /tmp/subscript-s86-round1b/append-report.py
git diff --check
git diff --stat
git status --short
git status --short --ignored -- REPORT.md
```

### Round 1b temporary instrumentation script

This script records the timer boundaries and counter definitions. None of its source edits remain in production files.

```python
from pathlib import Path
import shutil
base = Path('/tmp/subscript-s86-round1b')
for name in ['lib.rs', 'lir.rs', 'cemit.rs', 'root_storage.rs']:
    shutil.copyfile(Path('codegen/src') / name, base / name)
shutil.copyfile('REPORT.md', base / 'round1-report.md')
def edit(name, old, new):
    p = Path('codegen/src') / name
    s = p.read_text()
    assert s.count(old) == 1, (name, s.count(old), old)
    p.write_text(s.replace(old, new))
helper = '''
thread_local! {
    static S86_TIMES: std::cell::RefCell<std::collections::BTreeMap<&'static str, (u128, usize)>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static S86_START: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}
pub(crate) fn s86_record(name: &'static str, ns: u128) {
    S86_TIMES.with(|m| { let mut m = m.borrow_mut(); let v = m.entry(name).or_default(); v.0 += ns; v.1 += 1; });
}
pub(crate) fn s86_sum(name: &'static str) -> u128 {
    S86_TIMES.with(|m| m.borrow().get(name).map_or(0, |v| v.0))
}
macro_rules! s86_time {
    ($name:expr, $body:expr) => {{
        let start = std::time::Instant::now();
        let result = $body;
        s86_record($name, start.elapsed().as_nanos());
        result
    }};
}
'''
edit('cemit.rs', 'use std::fmt::Write as _;', 'use std::fmt::Write as _;\n' + helper)
edit('cemit.rs', '    verify_module(module).map_err(|errors| {', '    let s86_verify = std::time::Instant::now();\n    verify_module(module).map_err(|errors| {')
edit('cemit.rs', '    let program = Emitter::new(module)?.emit(require_main)?;', '    s86_record("pre.verify_module", s86_verify.elapsed().as_nanos());\n    let program = Emitter::new(module)?.emit(require_main)?;')
edit('cemit.rs', '''    verify_no_empty_aggregate(&program)?;
    verify_no_label_before_declaration(&program)?;
    Ok(program)''', '''    s86_time!("post.verify_no_empty_aggregate", verify_no_empty_aggregate(&program))?;
    s86_time!("post.verify_no_label_before_declaration", verify_no_label_before_declaration(&program))?;
    let post_ns = S86_START.with(|v| v.get().unwrap().elapsed().as_nanos());
    let measured = S86_TIMES.with(|m| m.borrow().iter().filter(|(k, _)| k.starts_with("post.")).map(|(_, v)| v.0).sum::<u128>());
    s86_record("post.remainder", post_ns.checked_sub(measured).unwrap());
    eprintln!("S86 post_plan_inclusive_ns={post_ns}");
    S86_TIMES.with(|m| { for (name, (ns, calls)) in m.borrow().iter() { eprintln!("S86 interval={name} ns={ns} calls={calls}"); } });
    Ok(program)''')
edit('cemit.rs', '        let root_storage = root_storage::plan(function, &emitter.layouts)?;', '''        let root_storage = s86_time!("plan.total", root_storage::plan(function, &emitter.layouts))?;
        S86_START.with(|v| v.set(Some(std::time::Instant::now())));
        let s86_before_coal = std::time::Instant::now();
        eprintln!("S86 root_slots={}", root_storage.slots.len());''')
edit('cemit.rs', '        let value_storage = coalesced_value_storage(', '''        s86_record("post.Body::new.pre_coalescing", s86_before_coal.elapsed().as_nanos());
        let s86_coal = std::time::Instant::now();
        let value_storage = coalesced_value_storage(''')
edit('cemit.rs', '        let declaration_scopes = declaration_scopes(', '''        s86_record("post.coalesced_value_storage", s86_coal.elapsed().as_nanos());
        let s86_decl = std::time::Instant::now();
        let declaration_scopes = declaration_scopes(''')
edit('cemit.rs', '        let mut delayed_declarations = HashSet::new();', '''        s86_record("post.declaration_scopes", s86_decl.elapsed().as_nanos());
        let s86_delayed = std::time::Instant::now();
        let mut delayed_declarations = HashSet::new();''')
edit('cemit.rs', '                                && declaration_can_use_instruction_assignment(&instruction.kind)', '                                && s86_time!("post.declaration_can_use_instruction_assignment", declaration_can_use_instruction_assignment(&instruction.kind))')
edit('cemit.rs', '''        Ok(Self {
            emitter,
            function,''', '''        s86_record("post.Body::new.delayed_declarations_exclusive", s86_delayed.elapsed().as_nanos() - s86_sum("post.declaration_can_use_instruction_assignment"));
        Ok(Self {
            emitter,
            function,''')
# Only the ordinary path executes for this fixture.
old = '''        let mut body = Body::new(self, function, false)?;
        body.emit_storage(out)?;
        body.emit_parameter_initializers(out)?;
        let _ = writeln!(out, "    goto b{};", function.entry.0);
        body.emit_graph(out)?;
        body.emit_unwind(out)?;
        verify_trap_consumption(function, &runtime_traps(function), &body.consumed_traps)?;'''
new = '''        let mut body = Body::new(self, function, false)?;
        s86_time!("post.Body::emit_storage", body.emit_storage(out))?;
        s86_time!("post.Body::emit_parameter_initializers", body.emit_parameter_initializers(out))?;
        let _ = writeln!(out, "    goto b{};", function.entry.0);
        s86_time!("post.Body::emit_graph", body.emit_graph(out))?;
        s86_time!("post.Body::emit_unwind", body.emit_unwind(out))?;
        let s86_traps = s86_time!("post.runtime_traps", runtime_traps(function));
        s86_time!("post.verify_trap_consumption", verify_trap_consumption(function, &s86_traps, &body.consumed_traps))?;'''
edit('cemit.rs', old, new)
edit('cemit.rs', '''            if matches!(function.kind, l::FunctionKind::Free) {
                let signature = self.wrapper_signature(function)?;''', '''            let s86_wrapper = std::time::Instant::now();
            if matches!(function.kind, l::FunctionKind::Free) {
                let signature = self.wrapper_signature(function)?;''')
# Limit the wrapper edit to the emit_function section.
p = Path('codegen/src/cemit.rs'); s = p.read_text()
a = s.index('    fn emit_function('); b = s.index('    fn emit_ordinary_function(', a)
part = s[a:b]; assert part.count('            Ok(())') == 1
part = part.replace('            Ok(())', '            s86_record("post.Emitter::emit_function.wrapper_text", s86_wrapper.elapsed().as_nanos());\n            Ok(())')
p.write_text(s[:a] + part + s[b:])
for name in ['emit_init_and_exports', 'emit_worker_adapters']:
    args = '&mut bodies' if name == 'emit_init_and_exports' else ''
    old = f'        self.{name}({args})?;'
    edit('cemit.rs', old, f'        s86_time!("post.Emitter::{name}", self.{name}({args}))?;')
edit('cemit.rs', '            render_allocation_metadata_definitions(self.module, &positions);', '            s86_time!("post.render_allocation_metadata_definitions", render_allocation_metadata_definitions(self.module, &positions));')
edit('cemit.rs', '            allocation_metadata_header: render_allocation_metadata_header(),', '            allocation_metadata_header: s86_time!("post.render_allocation_metadata_header", render_allocation_metadata_header()),')
# The lower_module call includes its own verification, reported separately.
edit('lib.rs', '''pub fn emit_c(module: &subscript_compiler::hir::Module) -> Result<CProgram, String> {
    reject_discovery_hir_for_c(module)?;''', '''pub fn emit_c(module: &subscript_compiler::hir::Module) -> Result<CProgram, String> {
    reject_discovery_hir_for_c(module)?;
    let s86_lower = std::time::Instant::now();''')
edit('lib.rs', '    cemit::emit_lir_c(&lir, true)', '''    eprintln!("S86 lower_module_ns={}", s86_lower.elapsed().as_nanos());
    assert_eq!(lir.functions.len(), 1);
    for f in &lir.functions {
        eprintln!("S86 instructions={} values={} blocks={}", f.blocks.iter().map(|b| b.instructions.len()).sum::<usize>(), f.values.len(), f.blocks.len());
    }
    cemit::emit_lir_c(&lir, true)''')
edit('lir.rs', '    if let Err(errors) = verify_module(&lowered) {', '    let s86_verify = std::time::Instant::now();\n    if let Err(errors) = verify_module(&lowered) {')
edit('lir.rs', '    Ok(lowered)\n}', '    eprintln!("S86 lowering_verification_ns={}", s86_verify.elapsed().as_nanos());\n    Ok(lowered)\n}')
# Plan intervals. Tail excludes destructor time, which belongs to plan.remainder.
edit('root_storage.rs', '''    let interference = value_interference_with(function, &held_to_exit)?;''', '''    let s86_graph = std::time::Instant::now();
    let interference = value_interference_with(function, &held_to_exit)?;
    crate::cemit::s86_record("plan.value_interference_with", s86_graph.elapsed().as_nanos());
    eprintln!("S86 plan_edges={} address_taken_values={}", interference.iter().map(|s| s.len()).sum::<usize>(), held_to_exit.len());
    let s86_slots = std::time::Instant::now();''')
edit('root_storage.rs', '    let mut words = 0u32;', '''    crate::cemit::s86_record("plan.slot_assignment", s86_slots.elapsed().as_nanos());
    let s86_tail = std::time::Instant::now();
    let mut words = 0u32;''')
edit('root_storage.rs', '    Ok(RootStoragePlan {', '    crate::cemit::s86_record("plan.tail", s86_tail.elapsed().as_nanos());\n    Ok(RootStoragePlan {')
# Sum the returned coalescing graph, without another interference computation.
edit('root_storage.rs', '''    Ok(interference)
}

fn value_interference_with''', '''    eprintln!("S86 coalescing_edges={}", interference.iter().map(|s| s.len()).sum::<usize>());
    Ok(interference)
}

fn value_interference_with''')
p = Path('codegen/src/root_storage.rs'); s = p.read_text()
a = s.index('fn value_interference_with('); b = s.index('pub(crate) fn managed_value_words', a)
part = s[a:b]
part = part.replace('    for block in &function.blocks {', '    let mut s86_max_live = 0usize;\n    for block in &function.blocks {', 1)
part = part.replace('        for instruction in block.instructions.iter().rev() {', '        s86_max_live = s86_max_live.max(live.len());\n        for instruction in block.instructions.iter().rev() {')
part = part.replace('''            for value in &instruction.invalidates {
                live.insert(origin(function, *value)?);
            }
        }''', '''            for value in &instruction.invalidates {
                live.insert(origin(function, *value)?);
            }
            s86_max_live = s86_max_live.max(live.len());
        }''')
part = part.replace('    Ok(interference)', '    eprintln!("S86 reverse_walk_max_live={s86_max_live}");\n    Ok(interference)')
p.write_text(s[:a]+part+s[b:])
fixture = Path('codegen/tests/boundary_scratch_breadth.rs').read_text()
probe = 'use std::fmt::Write as _;\nuse subscript_codegen::emit_c;\nuse subscript_compiler::{check_program, SourceFile};\n'
probe += fixture[fixture.index('fn mirror()'):fixture.index('fn address_plan(')]
probe = probe.replace('MAX_POSITIONS', 'width()')
probe += '''
fn width() -> usize { std::env::args().nth(1).unwrap().parse().unwrap() }
fn main() {
    assert!([16, 32].contains(&width()));
    let hir = checked(false);
    let start = std::time::Instant::now();
    let c = emit_c(&hir).expect("breadth fixture emits C");
    eprintln!("S86 width={} emit_c_total_ns={} source_bytes={}", width(), start.elapsed().as_nanos(), c.source.len());
    std::hint::black_box(&c);
    drop(c); drop(hir);
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: the successful call initializes the output.
    assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) }, 0);
    eprintln!("S86 max_rss_bytes={}", unsafe { usage.assume_init() }.ru_maxrss);
}
'''
Path('codegen/examples/s86_measure.rs').write_text(probe)
```

---

## Task A round 1 — the migration control stopped (at `f683a72`)

The interval query and the edge graph disagreed on
`a117-descriptor-literal-nullable-member` `main`, origins `%42` and
`%50`: interval `true`, graph `false`. Cause, measured with a LIR
dump: the form's live-in of block 4 is `[%50]`; the walk inserted
`%42` from the `invalidates` lists of three calls (`print` at lines
48, 52, 59 of the entry), which the lowering fills with every array
value in scope. `%42` was dead after block 3 and held its root slot
to the last call of blocks 4 and 6; the cross-block sets came from
the form, so the two liveness notions met inside one function.
Contract: §86.1 rule 2a at `75e96a6` (forced). Found by the coding
agent's control test, as §86.3 item 3 intends.

## Task A round 2 — the control stopped again (at `75e96a6`)

Under rule 2a the plan of 97 functions in 77 entries changes (the
agent's comparison, old walk before and after 2a; the entry list is
in the round's report and is reproduced by the control test). The
relation control then stopped at `a03-integer-literals` `main`,
origins `%0` and `%9`: interval `true`, graph `false`. Cause:
`%9 = LoadAddress(%8)`, `%8` an address with base `%0`;
`record_operand` inserts the base into the live set, the edge loop
adds edges to direct value operands only. Contract: §86.1 rule 2b at
`311e6f6` (forced). The interval query is the definition; the old
graph is corrected for the control.

## Task A round 3 — the control agreed; the review (at `311e6f6`)

Control under rules 2a and 2b: 181 corpus entries, 1,051 functions,
and the fixture at widths 8 and 16 — the interval relation equals
the corrected edge graph on every ordered origin pair; `value_slots`,
every slot field, both clear sets, coalesced storage, and the emitted
C are identical between the graph-driven and the interval-driven
derivations. No golden moved.

Stage table after Task A, release, Apple M2 (seconds):

| Stage | Width 16 | Width 32 |
|---|---:|---:|
| `Interference` build | 0.0013 | 0.0050 |
| slot loop | 0.0010 | 0.0098 |
| plan tail | 0.0001 | 0.0004 |
| `coalesced_value_storage` | 0.0008 | 0.0095 |
| `emit_c` total | 0.42 | 9.58 |
| maximum RSS | 30 MiB | 126 MiB |

Storage planning went from 11.1 s and 17.3 s (plan, coalescing) to
under 0.03 s together; `emit_c` from 27.2 s to 9.58 s; RSS from
1.59 GiB to 126 MiB. The remaining 9.5 s is Task B's region.

Fresh review (read-only): CRITICAL 0, MAJOR 2, MINOR 6.

- MAJOR: `Coalescing::try_merge` is O(members × members) per merge
  (12.6× measured for 5.4×); the parameter rules of `interferes`
  scan every block on a miss, invisible to the one-block fixture.
  Both assigned to Task A at `0f33896`.
- MINOR: test (d) did not isolate the function-parameter rule; the
  control's two derivations share the mention-set helpers
  (`record_operand`, `record_terminator`), so a defect there is
  invisible to the control (core principle 12; recorded, not
  changed); the `cfg(test)` record-only hook and the
  `REFERENCE_MODE` branch are control apparatus and leave in the
  landing commit; missing `///` on five crate-visible items; one
  dead guard.
- Fact for the record: after `thread_suspension_live_ins`
  (`codegen/src/lir.rs`), `Suspend.invalidates` is a subset of the
  successor's live-in, so the terminator mention in
  `record_terminator` is redundant under the form (reviewer). No
  divergence; the control cannot show it.

The first full gate on the round-3 tree stopped at `build`: four
dead-code warnings from the `lib test` control helpers. Round 4
lands the control.
