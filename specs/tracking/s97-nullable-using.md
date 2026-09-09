# §97 — a `using` binding can be null

Contract: `specs/blocks/compiler.md` §97. Origin: the owner's
2026-09-09 request to reconsider three decided restrictions on their
merits, under core principle 14.

Pin for every measurement: `de26008fc99a01815005ec845b0d66f7fef70c34`.
Host aarch64 macOS. rustc 1.95.0. tsc 5.9.2. node v24.18.0.
Command `target/debug/subscript run <file>`.

## Red at the pin

Every program uses a class `Res` that declares
`[Symbol.dispose](): void` and prints on open and on dispose, and a
factory `make(live: boolean): Res | null`.

| Program | exit | subscript | node |
|---|---:|---|---|
| `using r = make(false);` | 1 | S100 "a `using` initializer must be a non-null reference class that declares `[Symbol.dispose](): void`; narrow nullable values first" | `open:none body`, no disposal |
| `using r: Res \| null = null;` | 1 | the same S100 | `body`, no disposal |
| `using r = maybeNoHook();`, class without the hook | 1 | the same S100 | disposal fails at run time |
| `using r = null;` | 1 | S100 "cannot infer a type from `null`; annotate the declaration" | `body`, no disposal |
| the outer-`if` workaround | 0 | `open:r body dispose:r` | same |

One diagnostic covers three different problems, and it names the fix
for only one of them. §97.1 rule 2 splits it.

Node, for the mixed shape
`using a = make(true); using b = make(false);`:

```
open:r
open:none
body3
dispose:r
```

Only the live binding disposes, and reverse order reaches the null
binding first as a no-op.

## Why the restriction goes

C11 and §60.1 rule 3 state the rejection and say "narrow first, then
bind". Neither states a type-safety problem, because there is none:
the disposal is guarded, which is what JavaScript already does.

The workaround costs more than the record admits. An outer `if` is a
new block, so the resource dies at that block's end rather than at the
end of the scope the programmer meant. The rule shortens a lifetime to
express a check.
