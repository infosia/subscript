# A store to a shadow root reaches the MSVC output — `compiler.md` §210

Status: contract committed. Measurement and implementation are next.

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
`roots.v0 = 0;`. The trigger is not yet reduced.

The standalone reproducer is the emitted function of `Tile.area`, a
`main` that calls it with a stack object, and the stubs of
`subscript_rt_shadow_push`, `subscript_rt_shadow_pop`, and
`subscript_rt_trap` in a second file. With the stubs in the same file,
the program prints `9`.

With the frame `volatile`, in the reproducer only, the program prints
`9`. No cost is measured yet.

No other compiler was run: this machine has no clang. No other MSVC
version was run.
