<!-- §263 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 263. A raw registration is `unsafe`

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, findings R03
and R04. Selected by the owner on 2026-11-13.

Problem, read at `86bb58ff`. These public functions are safe Rust, and each
one keeps a raw address, a raw pointer, or a callback with its userdata
for a later use. A safe call with a pointer to a local value that ends
before that use makes a later read or write of memory that is not
valid. No hostile script is necessary.

| Function | Kept value | Later use |
|---|---|---|
| `Context::root_add` | an address and a word count | `prepare_collection`, `reachable_words` |
| `Context::shadow_push` | an address and a slot count | the same |
| `Context::set_trap_observer` | a callback and userdata | each trap |
| `Context::set_print_observer` | a callback and userdata | each `print` |
| `Context::set_diagnostics_observer` | a callback and userdata | each diagnostic |
| `Context::set_fn_table` | a table pointer | each call through the table |
| `Context::set_globals` | a global block pointer | each global access |
| `RunConfig::file_provider` (a public field) | a `FileProvider` with userdata | each file operation, through `Context::set_file_provider` |

The C exports of the same operations are `unsafe` and state the
lifetime (`runtime/src/ffi/memory.rs`). `Context::set_file_provider` is
`unsafe`, and the safe session constructor of `codegen/src/reload.rs`
calls it with the provider of `RunConfig` and does not establish its
safety condition.

### 263.1 Rules

1. A public function of the runtime or codegen crate that keeps a raw
   address, a raw pointer, or a callback userdata for a use after it
   returns is `unsafe`. Its `# Safety` section states what must stay
   valid, and until when.
2. Each function of the table becomes `unsafe`. `RunConfig::file_provider`
   is not a public field. A host sets it with the `unsafe` builder
   `RunConfig::with_file_provider`, whose `# Safety` section states the
   lifetime of the userdata and the callbacks.
3. Each call site gets an `unsafe` block with a `// SAFETY:` comment that
   names why the kept value stays valid.
4. A total check reads the public functions of the runtime and codegen
   crates. Each safe public function with a parameter of a raw pointer
   type, or of a type that holds one (`FileProvider`, an observer
   callback), is in an allowlist with a reason: the function does not
   keep the value after it returns. A new function that is not in the
   list fails the check.
5. No accepted program changes its output. No C export changes.

### 263.2 Acceptance

1. Red first, at the pin: the total check of rule 4 lists each function
   of the table.
2. After the change, the check passes. A firing control: a source text
   with a safe public function that takes `*mut u8` and is not in the
   list makes the check fail.
3. The check states its cost. It reads source text and builds nothing.
4. No corpus entry or golden changes.

### 263.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each path that passes a `FileProvider`
to `Context::set_file_provider`. Each path passes the value that the
`unsafe` builder set, or refuses a provider.

The total check of rule 4 found three more functions that keep a
pointer: `bind_callback`, `register_callback`, and `raise_exception`.
Each is `unsafe` now, under rule 1.

1. MINOR, realistic, fixed: the check did not read a function with a
   qualifier before `fn`, such as a safe `pub extern "C" fn`.
2. MINOR, realistic, recorded: §263.4 item 1.
3. MINOR, realistic, fixed: the `# Safety` text of three functions and
   the documentation of their C exports stated different lifetimes.
4. MINOR, realistic, fixed: some `// SAFETY:` comments were copies
   that did not give the reason at their site.
5. MINOR, realistic, fixed: the change added lines to
   `runtime/src/arrops.rs`, which is over the limit of §5 rule 2a.
6. MINOR, contrived, fixed: the `# Safety` text of
   `RunConfig::with_file_provider` did not state the owner thread.

### 263.4 Open

1. The check knows the record types that hold a raw pointer by a list:
   `FileProvider` and the observer callback types. It does not derive
   the list from the fields of each public record. No safe public
   function takes another such record now.
