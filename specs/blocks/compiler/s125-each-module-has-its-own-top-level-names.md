<!-- §125 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 125. Each module has its own top-level names

*(Added 2026-09-29.)* Origin: the seventh review of §124 found it; the
owner chose to fix it on 2026-09-29, and chose per-module names on
2026-09-29 after the §125 Phase Review.

Problem: the checker keeps several top-level declarations in
program-wide tables keyed by the bare name, so two modules that each
declare one name share one declaration or are rejected. Measured
through `subscript run` at `ef03101` and on the §125 working tree:

- `let x: i32` in `lib.ts` (100, `libBump()` adds 1) and in `main.ts`
  (7): the checker accepts; the program prints `main=101 lib=101`.
  JavaScript prints `main=7 lib=101`.
- `let x: string` in `lib.ts` and `let x: i32` in `main.ts`: S100 at
  `main.ts:2`, "the initializer expects string, got i32".
- A generic `pick<T>` in each module: `main=1 lib=1`; JavaScript prints
  `main=2 lib=1`. A generic `class B<T>` in each module:
  `main=100 lib=100`; JavaScript prints `main=7 lib=100`.
- A non-generic function or class of one name in two modules is
  rejected ("duplicate function name in the program", "duplicate class
  name in the program").
- A mirror `declare const K: string` and a program `let K: i32` in
  `main.ts`: `lib.ts`'s `K` resolves to `main.ts`'s global (S100).

`tsc` 5.9.2 accepts each two-module program, and `node` gives each
module its own names (`main=7 lib=100` for the first shape, measured).
Files with no `import` and no `export` are scripts in `tsc` and share
one scope (TS2451); a subscript program is a set of modules.

An enum is the working precedent: its scope item carries an `EnumId`,
and HIR, LIR, and both code generators use the id, so two enums of one
name in two modules run with JavaScript's values (`main=9 lib=1`).

The exported host entry is the one name the host sees. The ship tier
emits `subscript_export_<name>`, and the dev session resolves
`call_export(name)`, with no module qualifier (§5 symbol prefix, §59
rule 2 and rule 3). C has one symbol namespace.

### 125.1 Rules

1. A top-level declaration of a module (a module global, a function, a
   class, a generic function, a generic class) binds its name in that
   module only. Another module reaches it only through an `import`. Two
   modules can declare one name; each keeps its own declaration, its
   own storage, and its own value, as JavaScript gives.
2. A declaration has an identity that is not its name, as an enum has
   (`EnumId`). The file scope, the checker tables, HIR, LIR, both code
   generators, the interpreter, and hot reload find a declaration by
   that identity. No pass finds a top-level program declaration by its
   bare name. A generic instance key, a static member key
   (`Class.member`), and a worker entry key use the identity too, so
   `pick<E>` over two enums named `E` is two instances.
3. A mirror (`.d.ts`) declaration stays in the ambient scope. A
   top-level declaration of one name in a module hides it in that
   module only; another module still resolves the name to the mirror
   declaration. A foreign function keeps its C symbol name: C has one
   symbol namespace.
4. A host entry name is unique in the program. A host entry is an
   exported function that gets a `subscript_export_<name>` symbol
   (`hir::Function::host_entry_trap_sites` is `Some`). A second host
   entry of one name in any module is S017, at the second declaration
   in module order. An exported function that is not a host entry is
   not affected. This diverges from `tsc`, which accepts the program;
   C14 records it.
5. A diagnostic, a trap message, the LIR text, and a reload message
   print the source name. Where two declarations of one name can
   appear in one message, the message names the module too.
6. The one-namespace-per-module rules for one module do not change: a
   second declaration of one name in one module is S017 (§82.2).

### 125.2 Acceptance

1. Measure first: every corpus source and every example checks and runs
   as before; no `.expected` golden moves. The LIR text snapshot does
   not move, or the round reports each moved line.
2. Accept entries, each a two-module directory entry with a golden,
   `js-comparable`, on all three engines and `node`: a module global of
   one name (the measured shape), a generic function, a generic class,
   a non-generic function, a non-generic class, a static member of two
   classes of one name, a generic instance over two enums of one name,
   and a mirror `declare const` hidden by a program global in one
   module. Each is Red at `ef03101` (rejected or a different value) and
   the round records the Red output.
3. A reject entry pins rule 4: two modules each export a host entry of
   one name. Its header states what `tsc` does, measured, and cites
   C14. The round states what `ef03101` does with it.
4. A test pins that the dev JIT gives two module globals of one name
   two storage slots (the JIT keyed global slots by name at `ef03101`).
5. Rule 2 has a total check: a test builds every declaration kind of
   rule 2 twice under one name in two modules and runs the program on
   every engine. A new declaration kind is added to that test.

### 125.3 Open

A module-qualified host entry symbol, so two modules can export host
entries of one name, was the planned next form (owner, 2026-09-29).
*(Withdrawn 2026-09-29, owner decision: §129 makes the entry module
the host API, so rule 4 and this plan are replaced.)*
