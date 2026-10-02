# §150 — Postfix update

The HIR assignment form carries `UpdateKind::Prefix` or `UpdateKind::Postfix` for numeric updates.
The shared LIR lowering saves the old operand, writes the new operand, and selects the result.
The dev JIT, C emitter, and interpreter consume that LIR.
The existing place preparation evaluates the receiver, array, and index once before the read.

## Golden search

A TypeScript AST scan covered corpus sources, examples, and TypeScript and JavaScript documentation blocks.
A text scan also covered generated-reference examples and their generators.
No existing program uses a postfix update in value position.
No existing golden changes.

## Red evidence

The dev binary built from `6c8d3cdb` runs `a326-postfix-update` with the following output.
Node v24.18.0 with TypeScript 5.9.2 produces the committed golden.

| Case | Pinned dev | Node / golden |
|---|---|---|
| Local increment | `1 1` | `0 1` |
| Local decrement | `4 4` | `5 4` |
| Field | `1 1` | `0 1` |
| Index | `11 11` | `10 11` |
| Loop condition | `2 3` | `3 4` |
| f64 | `2.5 2.5` | `1.5 2.5` |
| Prefix control | `1 1` | `1 1` |
| Module global | `21 21` | `20 21` |
| Captured object field | `31 31` | `30 31` |
| u8 maximum | `0 0` | `255 0` |
| i64 | `41 41` | `40 41` |
| Side-effecting index | `12 12 1` | `11 12 1` |

Mutable local captures are rejected by S009.
The lambda witness captures a const object local and updates its field.
The u8 corpus witness masks the stored value explicitly for Node comparison.
The execution unit tests also check the stored u8 wrap without that mask.

## Tests

The checker tests both operators, both result modes, and both expression positions.
Each execution form tests prefix and postfix values, statements, target evaluation order, and u8 wrap.
The new corpus test compares all three forms with the golden.

The final execution unit tests pass: four tests in 0.44 seconds.
The JS corpus tests pass: eleven tests.
The dev and ship corpus sweep and the interpreter corpus sweep pass.
`cargo fmt --check` and `git diff --check` pass.

The compiler/codegen offline locked test command stops at one compiler unit-test failure.
`language_reference::tests::generated_ai_references_are_byte_identical` rejects the stale corpus index.
The compiler unit suite reports 371 passes and one failure.
The remaining compiler integration tests do not run after that failure.

## Required scope change

Regenerate `generated-docs/corpus-index.md` to add the a326 corpus row.
The handoff permits changes to generators only under `generated-docs/`.
It requires a stop when another file needs a change.
The orchestrator regenerated `generated-docs/corpus-index.md` through `generate-api-reference`; it adds the `a326` row.

## Landing gate

```text
gate full 39aba33830d40adfe37e46f5f4d8d4e9ad2be756 dirty:18 debug 2226/0/3 release 2223/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
