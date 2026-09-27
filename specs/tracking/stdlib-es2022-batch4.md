# ES2022 stdlib batch 4 — Error classes, `Error.prototype.toString`, URI functions — evidence

Contract: `specs/blocks/stdlib.md` §19 (Rev 18); `compiler.md` §115.1
and `collisions.md` C6 amended. Date: 2026-09-27. Host: arm64 macOS.

`tsc` 5.9.2 (lib `ES2022` + `ESNext.Disposable`, strict) accepts the
four constructors, `toString()`, the four URI functions, and
`e instanceof URIError` / `RangeError` in a `catch` (exit 0).

`node` v24.18.0:

```
RangeError m RangeError: m true RangeError
ReferenceError m ReferenceError: m true ReferenceError
EvalError m EvalError: m true EvalError
URIError m URIError: m true URIError
Error m Error: m true Error
TypeError m TypeError: m true TypeError
SyntaxError m SyntaxError: m true SyntaxError
"x"            (name "", message "x")
N              (name "N", message "")
"a b" a%20b a%20b
"héllo" h%C3%A9llo h%C3%A9llo
"😀" %F0%9F%98%80 %F0%9F%98%80
"a&b=c/d?e#f" a%26b%3Dc%2Fd%3Fe%23f a&b=c/d?e#f
"-_.!~*'()" -_.!~*'() -_.!~*'()
";,/?:@&=+$#" %3B%2C%2F%3F%3A%40%26%3D%2B%24%23 ;,/?:@&=+$#
"" (empty) (empty)
"%" %25 %25
U+2028 %E2%80%A8 %E2%80%A8
a%20b "a b" "a b"
%E2%82%AC "€" "€"
%F0%9F%98%80 "😀" "😀"
%3B%2F%3F ";/?" "%3B%2F%3F"
% %zz %C3 %ED%A0%80 %C0%80 %FF → URIError (both functions)
```

`node`'s `URIError` message is `URI malformed`.

## Before the implementation

The checker at `2b5bafb`: the four new constructors and `toString()` on
them S016; `instanceof` a new class S100; `toString()` on `Error`,
`SyntaxError`, `TypeError` S018; the four URI functions S016.

## Landing

One table, `ERROR_CLASSES` (`compiler/src/check/exception.rs`), names
the seven classes and their tags; name resolution, construction,
`instanceof`, and the API reference read it. The URI functions and
`toString()` are one HIR family, `TextFn`, lowered on the dev JIT, the
ship C, and the interpreter; `toString()` reuses the runtime's report
routine; a decode helper `throw`s `URIError` on the §115 path, so the
call is a raise site of its caller. `a267-error-classes-and-uri`
equals `node` byte for byte (899 bytes) on all three engines. The LIR
text snapshot gained 336 intrinsic declaration lines, and in eight
instructions `instanceof Error`'s tag bound moved from 3 to 7.

A fresh review checked every byte 0x00–0x7F through both encoders, 24
malformed decode inputs, 7 × 7 `instanceof` both ways, the byte offset
in the message, propagation through helpers, an uncaught `URIError` at a
host entry (trap 29), and that `repeat(-1)`, `toFixed(101)`, and radix 1
still trap: equal to `node` on both tiers. No CRITICAL or MAJOR. MINOR,
fixed: §115 still enumerated three classes in rules 1, 3, 5 and §115.2
rule 1 (now "the Error family"). MINOR, open:

- Each `decodeURI`, `decodeURIComponent`, and `Error.toString` call site
  creates a helper function, and every function signature enters the
  reload declaration hash (`codegen/src/reload.rs`), so a body edit that
  adds such a call refuses a hot swap. The `JSON.parse` helpers already
  had this shape. It works against invariant 3.
- A successful decode runs the decoder twice (a failure probe, then the
  decode); the cost is not measured.
- The runtime symbol of a `TextFn` is spelled in `hir::TextFn::symbol()`
  and again in `codegen/src/lir.rs` (the JSON rows share the shape).
- `mod text;` sits after the test module in `compiler/src/check/mod.rs`
  and `codegen/src/interpreter.rs`; the C emitter's `Text` arm reports
  `JSON.{name}`; the API summary of the encoders mentions decoding; the
  new exports are in `runtime/src/ffi.rs` (7,682 lines) rather than in
  `runtime/src/uri.rs` and `runtime/src/exception.rs`.

`gate full 2b5bafb dirty:32 debug 1732/0/2 release 1729/0/2 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
