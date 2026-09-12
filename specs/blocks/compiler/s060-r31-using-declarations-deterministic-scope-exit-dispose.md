<!-- §60 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 60. R31 — `using` declarations: deterministic scope-exit dispose

Origin: downstream request R31, 2026-08-16, at pin `e8e01d9`
(their pin `dae6e10`). Their script-driven programs hold 577
`dispose()` call sites across two suites; `a27-host-compute.ts`
alone ends in a 16-line reverse-order release tail with two
early-return failure paths, and every program that reads a GPU
result crosses at least one `await` between creation and disposal.
Three owner decisions (2026-08-16) pre-date this contract and are
its starting point: nullable `using` is rejected; a trap does not
run dispose; the cleanup member is `dispose`, with
`[Symbol.dispose]` as the hook.

Measurements at the pin, on this host:

1. The pinned `swc` parses `using` declarations and
   `[Symbol.dispose]()` class members. Both reach the checker and
   fail with S100 ("nested declarations are not in the decided
   surface"; "computed method names are not decided"). The parser
   needs no change.
2. `tsc` 5.9.2 with `lib: ["ES2022"]` fails the hook member with
   TS2318/TS2550; with `lib: ["ES2022", "ESNext.Disposable"]` the
   same program exits 0.
3. Under `node` v24.18.0 (exit 0): bindings dispose at block end
   in reverse declaration order; a return expression evaluates
   before the disposals; an early return disposes only the
   bindings in scope at that point; a loop disposes per iteration,
   and `break` disposes the current iteration's bindings; an
   `async` function that suspends across `await` disposes at
   completion, after the resume.

### 60.1 Rule

1. A reference class can declare the member
   `[Symbol.dispose](): void` — non-static, non-async, no
   parameters, `void` return. The member is a method under a
   reserved internal name that no source identifier can spell. A
   value class or a descriptor class that declares it fails with
   S100. Every other computed member name stays
   rejected, and the explicit call spelling `x[Symbol.dispose]()`
   stays rejected — a class that wants a manual spelling declares
   an ordinary method (the downstream uses `dispose()`).
2. `using x = expr` declares an immutable binding in a function
   block scope. A module-level `using` and a `using` in a `for`
   head fail with S100. A `using` statement can declare several
   bindings; their disposal order is the reverse binding order.
3. The initializer's static type must be a reference class that
   declares the hook. Any other type fails with S100. *(§97,
   2026-09-09: the nullable case is accepted, and the guard runs at
   each exit. The hook requirement in the first sentence stands; the
   rest of this rule retires.)*
4. Dispose runs at every exit of the owning scope: the natural
   end, `return`, `break`, `continue` that leaves the scope, and
   the end of each loop iteration. Order: reverse declaration
   order within a scope; the innermost scope first when one exit
   leaves several scopes. A `return` expression evaluates into a
   synthesized local before the disposals run. (All measured
   under `node`, §60 item 3.)
5. A coroutine suspension is not a scope exit. A frame that
   suspended across the binding disposes at completion (measured
   under `node`).
6. A trap does not run dispose (owner decision; §18.1b, no
   rollback).
7. `await using` fails with S100. Every downstream disposal is
   synchronous.
8. The rewrite is checker-complete. After checking, a `using`
   declaration is a `const` binding plus hook-method calls
   inserted at the scope exits; the HIR contains only forms that
   exist today. No new HIR node, no codegen, runtime, or prelude
   change; the tiers agree by construction.
9. The language adds no aliasing or use-after-dispose rule. The
   hook is an ordinary method call; the class owns its behavior
   after disposal.
10. `tsconfig.json` `lib` gains `"ESNext.Disposable"`. No other
    gate changes.

`collisions.md` gains C11: JS skips a null `using` binding and
runs disposal during throw-unwind; subscript rejects the nullable
binding at check time and does not dispose on a trap.

### 60.2 Changes by site

- `compiler/src/check/mod.rs`, `compiler/src/hir.rs`: accept the
  hook member on reference classes under the reserved internal
  method name; validate its shape at the declaration.
- The statement checker: check `using` declarations, track live
  bindings per scope, and insert the hook calls at every scope
  exit, including the synthesized return local.
- `tsconfig.json`: the `lib` line.
- No parser, codegen, runtime, or prelude change.

### 60.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: record the S100 diagnostics for
the corpus entries below before the implementation.

1. `corpus/accept/a138-using-dispose.ts` + `.expected` (golden
   from the dev JIT; ship byte-identical): a reference class whose
   hook prints its label; one block with two bindings (reverse
   order); a helper with a return value (the value line prints
   before the dispose line); an early return; a loop with `break`
   (per-iteration disposal). The printed sequence must equal the
   `node` measurement shape from §60 item 3.
2. `corpus/accept/a139-using-async.ts` + `.expected`: an
   `export async function main` with a binding that crosses an
   `await`; the golden pins the resume line before the dispose
   line.
3. *(`corpus/reject/r131-using-nullable-init.ts` held a nullable
   initializer at S100. It retired 2026-09-09 by §97, which accepts
   the form.)* `corpus/reject/r132-await-using.ts`:
   `await using`, S100.
   `corpus/reject/r133-using-without-dispose.ts`: an initializer
   type without the hook, S100.
4. Unit tests in the same commit: module-level `using` fails; a
   `for`-head `using` fails; a value-class hook member fails; a
   non-hook computed member name still fails; the explicit
   `x[Symbol.dispose]()` spelling still fails; a multi-binding
   `using` statement disposes in reverse order.
5. Counts: accept single files 136 → 138; accept source files
   138 → 140; rejects 126 → 129; goldens 137 → 139. The generated
   docs regenerate byte-exact.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate with
   the new `lib` entry; every pre-existing golden and `.expected`
   byte-identical.
