# `join` formats a string-literal alias — `compiler.md` §216

Status: contract.

## Measurement at the pin `673b1113`

`const ms: Mode[] = ["on", "off"]; print(ms.join(","));` gives S014
"`join` formats elements by the Q14 interpolation rules; `Mode` elements
are not interpolatable (Q22)". `` print(`${m}`) `` with `m: Mode` is
accepted.

The §204 text names a new format kind in the runtime formatter. The
checker already rewrites `` `${xs}` `` to `xs.join(",")`, and each tier
has the string form of one alias value, so a rewrite needs no change in
the three tiers.
