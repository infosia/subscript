# Tracking: cycles through a local lambda or a list, and the class hash (§306)

Contract: `specs/blocks/compiler/s306-cycles-through-a-local-lambda-or-a-list-and-the-class-hash.md`.
Origin: §290.4 item 1, §253.4 item 1, §294.4 item 1. On 2026-11-15 the
owner told Claude to continue with the open items.

## Red

| Entry | Contract pin | `tsc` 5.9.2 | After |
|---|---|---|---|
| `r516` | accepted | TS7023 at `a`, `inner`, `b` | three S100 |
| `r517` | accepted | TS7023 at `a` | one S100 |
| `r518` | accepted | TS2368 | one S100 |

The reload tests that edit `callback_lifetime` or `boundary_header` of a
mirrored class are accepted by the pin hash and refused after the
change.

## Rule 1c, measured by the Phase Review

`tsc` accepts a call through a value that holds a lambda outside the
cycle: `(c ? a2 : b)()`, `[a2, b][i]()`, `const f = table[0]; f()`, and
a dispatch table of state functions. It gives TS7023 when each source of
the value is in the cycle: `[b]`, `[b, b]`, `[...base]`, `[[a]][0]`,
`const h = b`, `c ? b : b`.

## Cost

A chain `const fN = () => { return fN-1(); };`, debug, median of three:
N = 1000, 0.231 s at the pin and 0.231 s after; N = 3000, 0.906 s and
0.915 s. The accept corpus checks in 3.32 s at the pin and 3.33 s after.
