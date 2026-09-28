// corpus: trap/t73-boxed-local-narrowing-call
// purpose: A getter clears a field of a boundary box held in a local.
// exercises: shared-location-narrowing, boundary-box, getter
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the boxed local field read
class H { t: SGPUProbeColorTargetState | null = new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3); }
const h: H = new H();
function clear(): void { const p = h.t; if (p !== null) { p.blend = null; } }
class K { get acc(): i32 { clear(); return 0; } }
export function main(): void {
  const t = h.t;
  if (t !== null) {
    if (t.blend !== null) {
      const { acc } = new K();
      print(`${t.blend.colorOperation}`);
    }
  }
}
