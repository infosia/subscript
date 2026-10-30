# A store to a shadow root reaches the MSVC output — `compiler.md` §210

Status: open. The cost method of §210.4 rule 3 is corrected. The `volatile` scrutinee is measured next.

## Measurement at the pin `c7bb7e55`

`tools/gate.sh full` on Windows, MSVC 19.44.35222: debug 2947 passed and
1 failed, release 2944 passed and 1 failed. Both failures are
`a383-exhaustive-enum-switch: ship-C-AOT run failed` with exit code
`0xc0000005`.

Reductions of the script, on the ship tier:

| Method body | Result |
|---|---|
| `switch` over `this.shape`, a return in each case | `0xc0000005` |
| `if (this.shape === Shape.Square)`, then a return | prints `9` |
| no field read | prints `9` |

Optimized assembly of `Tile.area`, standalone. `a0` arrives in `rdx`:

    xor  ebx, ebx
    mov  QWORD PTR roots$[rsp], rbx
    call subscript_rt_shadow_push
    mov  QWORD PTR roots$[rsp], rbx
    mov  rcx, rdi
    mov  edx, DWORD PTR [rbx]

In the failed function, every `switch` successor begins with
`roots.v0 = 0;`.

The standalone reproducer is the emitted function of `Tile.area`, a
`main` that calls it with a stack object, and the stubs of
`subscript_rt_shadow_push`, `subscript_rt_shadow_pop`, and
`subscript_rt_trap` in a second file. With the stubs in the same file,
the program prints `9`.

With the frame `volatile`, in the reproducer only, the program prints
`9`. No cost is measured yet.

No other compiler was run: this machine has no clang. No other MSVC
version was run.

## Reduced trigger

The defect needs no struct and no `goto`. With `push` and `pop` in a
second file, this function fails at `/O2`:

    __declspec(noinline) int test(int* arg) {
        int* slot = 0;
        push(&slot);
        slot = arg;
        switch (*slot) {
        case 1:  slot = 0; pop(); return 9;
        default: slot = 0; pop(); return 0;
        }
    }

| Change | Result |
|---|---|
| a struct member in place of the local | `0xc0000005` |
| remove the initializer | `0xc0000005` |
| `memset` in place of the clear | `0xc0000005` |
| keep the clear in one successor only | prints `9` |
| remove every clear | prints `9` |
| remove the first address use | prints `9` |
| remove the call after the clear | prints `9` |
| `if` in place of `switch` | prints `9` |
| no branch | prints `9` |
| `slot` is `volatile` | prints `9` |

`&roots.env_v0` through a typed pointer shows the defect too. With a
`volatile` frame, that access still fails, because the cast removes the
qualifier. Candidate A therefore does not cover that storage as written.

## Cost of candidates A and B

Ship column of the cross-language benchmark, compiled with `cl`, median
in ms, 11 samples. A measurement copy of the runner used `cl`, because
the runner selects clang for the ship subject.

| Workload | Pin | A | B |
|---|---|---|---|
| fib-recursive | 3.5186 | 3.5230 | 3.5490 |
| fib-loop | 21.0964 | 21.7468 | 22.0341 |
| mandelbrot | 78.9971 | 76.9674 | 77.9725 |
| primes | 33.2701 | 33.2566 | 34.1495 |
| sort | 22.7659 | 26.7771 | 25.0954 |
| tree | 115.4227 | 117.2487 | 114.8184 |
| queen | 23.8509 | 24.0538 | 23.1838 |
| particles | 90.9711 | 116.3063 | 118.4842 |
| callbacks | 57.9118 | 59.7351 | 59.3565 |
| collect | 59.6419 | 62.5629 | 56.9723 |

A: `sort` +17.62%, `particles` +27.85%. B: `sort` +10.23%, `particles`
+30.25%. The limit is 5%. No production change landed.

## Narrower forms, correctness only

| Form | Reduced function | Emitted `Tile.area` |
|---|---|---|
| no change | `0xc0000005` | `0xc0000005` |
| each clear is a store through `volatile`, reads unchanged | prints `9` | prints `9` |
| `_ReadWriteBarrier()` before the `switch` | `0xc0000005` | prints `9` |
| `_ReadWriteBarrier()` before each clear | `0xc0000005` | not run |

The barrier does not close the trigger. The `volatile` clear has no
cost measurement yet.

## Cost of the `volatile` clear

The prototype routed 23 clear sites through one function, on MSVC only.
Ship column, compiled with `cl`, median in ms, 11 samples. No production
change landed.

| Workload | Pin | Prototype | Change |
|---|---|---|---|
| fib-recursive | 3.5186 | 3.5400 | +0.608% |
| fib-loop | 21.0964 | 21.1768 | +0.381% |
| mandelbrot | 78.9971 | 77.2457 | -2.217% |
| primes | 33.2701 | 33.5896 | +0.960% |
| sort | 22.7659 | 22.7254 | -0.178% |
| tree | 115.4227 | 118.0332 | +2.262% |
| queen | 23.8509 | 23.3071 | -2.280% |
| particles | 90.9711 | 100.2837 | +10.237% |
| callbacks | 57.9118 | 67.9834 | +17.391% |
| collect | 59.6419 | 80.1281 | +34.349% |

Three workloads exceed the 5% limit by this table. See "Correction" below. `&roots.env_v0`
through a typed pointer printed `9` with this form, so the form was
correct.

## A `volatile` scrutinee, correctness only

The form copies the `switch` value to a `volatile` local. The clears
do not change. `cl /O2`, with the stubs in a second file:

| Shape | Plain | `volatile` scrutinee |
|---|---|---|
| one case and `default` | `0xc0000005` | prints `9` |
| three cases and `default` | `0xc0000005` | prints `9` |
| a statement before each clear | `0xc0000005` | prints `9` |
| six cases and `default` | prints `9` | prints `9` |
| emitted `Tile.area` | `0xc0000005` | prints `9` |

The six-case shape does not show the defect at `/O2`. No cost is
measured yet.

## Correction: the cost tables above do not separate cost from noise

The tables compare one pin run with a later run. No run alternates the
two binaries. The `volatile` scrutinee round showed the error:

- The ten workloads emit 0 `switch`. For them, the form changes only a
  macro definition, so the expected change is zero.
- The same round reported particles +12.244%, collect +10.536%,
  mandelbrot +6.674%, fib-loop +6.650%, and queen +5.052%.
- The C subject does not change in any round. Its median still moved:

| Workload | A | B | `volatile` clear | `volatile` scrutinee |
|---|---|---|---|---|
| tree | +6.1% | +12.5% | +13.7% | +16.0% |
| callbacks | +2.8% | +3.9% | +41.5% | +4.2% |
| collect | +29.8% | +22.3% | +106.0% | +58.7% |

The earlier statements that A, B, and the `volatile` clear exceed the
limit are not established. The two binaries must alternate in one
session, as the §194 cost measurement did.
