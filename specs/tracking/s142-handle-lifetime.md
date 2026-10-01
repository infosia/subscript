# §142: Host handle lifetime

Contract: `specs/blocks/compiler/s142-a-handle-the-script-keeps-is-the-hosts-to-keep-alive.md`, rules 1, 2, and 5.

`runtime/src/host_header.rs` adds the lifetime text as the last paragraph of the `subscript_main_entry` documentation block.
The program header carries that text from its copy of the runtime header.
The emitter adds no second copy. Each header carries the text once.
Both generator tests pin the full documentation block and count the §142 citation.
The lifetime text uses "A handle that the host passes to a script transfers no ownership."
The runtime generator regenerated `runtime/include/subscript_runtime.h`.

`corpus/accept/a137-handle-entry-param.ts` adds §142 to its `questions` line.
The document generator regenerated `generated-docs/corpus-index.md`.
No compiler or runtime behavior changes. No `.expected` file changes.

Validation: all 13 runtime header tests passed, with byte-identical regeneration.
The program header test and allocation metadata header regeneration test passed.
The generated document byte-identity test passed.
The pinned `cargo fmt --check` passed. The workspace all-target build passed with zero warnings.
