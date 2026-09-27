<!-- §119 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 119. A synthesized helper has no reload slot

*(Added 2026-09-27.)* Origin: the review of stdlib §19. The owner chose
to fix it before the next stdlib batch.

Problem: the checker synthesizes a helper function per call site of
`JSON.stringify`, `JSON.parse`, `decodeURI`, `decodeURIComponent`, and
`Error.prototype.toString` (`compiler/src/check/json.rs`,
`compiler/src/check/text.rs`). Its name counts the functions made so far
(`self.functions.len()`), and every function signature enters the
reload declaration hash (`codegen/src/reload.rs`, `declaration_hash`).
So a body edit that adds one such call changes the hash, and §8.2
refuses the swap: the program must restart. Invariant 3 makes the
iteration tier a goal, and `JSON.parse` is an everyday call.

The hash cannot simply leave the helpers out. The dev JIT numbers its
indirection-table slots in HIR order, and helpers are ordinary free
functions interleaved with user functions (`reserve_slots`,
`codegen/src/lower/mod.rs`). A new helper moves every later slot, and
retained old code (a lambda in Context state, a native registration, a
function value's wrapper) calls through the current table by its old
slot number. Without the hash, it would call another function, possibly
with another signature.

Lambdas already have the sound form: no table slot, a direct call
within their own generation, and no name or signature in the hash
(`codegen/src/lir/lambda.rs`, `codegen/src/lower/func/call.rs`). A
helper is reachable only from bodies of its own module: it is not
exported, and its name (`[[...]]`) cannot be spelled in source, so no
function value refers to it.

### 119.1 The rule

1. **A helper is marked, not named.** The HIR carries an explicit fact
   on each function the checker synthesizes: JSON stringify and parse
   helpers, the URI decode helpers, the `Error.toString` helpers, and
   every later one. No stage reads the `[[` prefix to decide.
2. **A helper has no indirection-table slot.** The dev JIT calls it
   directly within the generation that compiled the caller, as it
   calls a lambda body. A user function's slot does not depend on how
   many helpers exist or where.
3. **A helper is not in the declaration hash.** Its body depends only on
   hashed declarations (class fields and types) and on its call site, so
   a body edit that adds, removes, or reorders a helper is an accepted
   swap.
4. **A helper takes no carrier parameter** (§118.1 rule 1). Its escape
   facts are therefore constant, and leaving it out of the hash does not
   weaken §118.1 rule 10. A helper with a carrier parameter is an
   internal error at lowering.
5. A retained old caller keeps calling its own generation's helper, as
   a retained lambda keeps its own body. This is the cost that §8.2
   already states for lambdas.
6. **The Error-family class is declared in every module.** It is
   created on first use today (`compiler/src/check/exception.rs`), and
   `JSON.parse`, the URI decoders, and `new Error` all reach that first
   use. A class enters the declaration hash, so the module's first such
   call refused the swap: measured, a body edit that added
   `JSON.parse<i32>("42")` to a module with no Error use was refused
   with "class Error". The class is declared in every module as the
   first class (id 0), before any user class is declared, so its id and
   every user class id are fixed by the declarations and a body edit
   cannot add or move it. No pass rewrites a class id after the
   declarations. *(Added 2026-09-27 by the review. The first form put
   the class after every user class, which needed a pass that rewrote
   every `ClassId` in the HIR by hand; a field added later to a HIR type
   would have escaped that pass without a compiler error.)*

### 119.2 Sections this one amends

- §8.2: the declaration hash covers every user function; a synthesized
  helper is outside it.

### 119.3 Tests (pre-registered)

- A reload whose new source adds a `JSON.parse`, a `JSON.stringify`, a
  `decodeURI`, and an `Error.toString` call inside an existing body is an
  accepted swap, and the new calls run. The same edit at `5dce95b` is a
  rejected swap (record it).
- A retained old lambda that calls a helper keeps working after a swap
  that renumbers the new generation's helpers.
- The declaration hash of a module is equal with and without an extra
  helper call in a body; it still changes for a signature edit (firing
  control).
- A reload whose new source adds the first `JSON.parse`, the first
  `decodeURI`, and the first `new Error` to a module that used none of
  them is an accepted swap.
- A helper with a carrier parameter, built directly as HIR, fails
  lowering with the internal error of rule 4.

### 119.4 Goldens that move

The LIR text snapshot can move if it prints the helper fact or the
class table. No `.expected` golden moves. If one does, stop and report.

### 119.5 Open items outside this section

- `can_raise` (§115.6 rule 3) is derived from bodies and is not in the
  hash. A body edit that makes a callee start raising can reach a
  retained caller compiled without a raise edge at that call.
- The first use of a new generic instance (`f<T>`) adds a declaration,
  so it refuses a swap. An instance can be a function value, so rule 2
  does not apply to it.

### 119.6 Exit criteria

1. The tests of §119.3 pass on the dev JIT.
2. `tools/gate.sh full` is green.
3. A fresh review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
