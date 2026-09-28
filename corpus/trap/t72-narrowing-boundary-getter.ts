// corpus: trap/t72-narrowing-boundary-getter
// purpose: A getter clears a narrowed boundary box before a field store.
// exercises: shared-location-narrowing, boundary-box, getter, null-narrowing
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the boundary box read
class H { b: SGPUProbeBlendState | null = new SGPUProbeBlendState(5, 6); }
const h: H = new H();
class K { get acc(): i32 { h.b = null; return 2; } }
export function main(): void {
  const k = new K();
  if (h.b !== null) { const { acc } = k; h.b.colorOperation = 9; print("stored"); }
}
