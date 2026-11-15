# Tracking: the ship tier asserts the string view of a bound callback (§298)

Contract: `specs/blocks/compiler/s298-the-ship-tier-asserts-the-string-view-of-a-bound-callback.md`.
Origin: §183.4 item 1. On 2026-11-15 the owner told Claude to do each
item of a list of open runtime defects.

## Form

- The binder writes the host view type and its member names into the
  callback provenance line of the mirror:
  `// @subscript-c-callback typedef="SubLogCallback" aggregate="SubStringView" data="data" len="len"`.
  The two generated mirrors in the tree (`corpus/interop/`,
  `examples/engine/`) change in this line only.
- The HIR and the LIR callback records carry the view type and the
  member names (`compiler/src/provenance.rs`, `compiler/src/hir.rs`,
  `compiler/src/lir.rs`, `codegen/src/lir.rs`).
- `codegen/src/cemit/marshal.rs` and `emitter.rs` emit one assertion
  for each host view type, before the first trampoline cast:

```c
_Static_assert(
    sizeof(HostView) == sizeof(subscript_callback_string_view) &&
    _Alignof(HostView) == _Alignof(subscript_callback_string_view) &&
    offsetof(HostView, data) == offsetof(subscript_callback_string_view, data) &&
    offsetof(HostView, len) == offsetof(subscript_callback_string_view, len) &&
    sizeof(((HostView*)0)->data) == sizeof(((subscript_callback_string_view*)0)->data) &&
    sizeof(((HostView*)0)->len) == sizeof(((subscript_callback_string_view*)0)->len),
    "callback string-view layout: HostView");
```

- `codegen/tests/callback_view_layout.rs`: the emitted C with the
  matching view compiles, and a view with the two members in the other
  order fails at the assertion (0.09 s).
- No `cl` run on this host.

## Phase Review

One fresh review: one MAJOR and four MINOR findings, each fixed; see
§298.3. The MAJOR changed the contract (rule 1a): a callback record
from an older binder is now a checker diagnostic, not an internal
error of the ship build.

The first full gate failed in `coroutine_and_measurement_lir_text_matches_goldens`:
18 callback field lines of the LIR text snapshot now print the view
type and the member names. No other line changed. The snapshot was
captured again with `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`.
