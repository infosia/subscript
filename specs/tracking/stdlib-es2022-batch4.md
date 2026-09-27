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
