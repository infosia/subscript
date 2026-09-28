// corpus: trap/t74-boxed-local-narrowing-alias-store
// purpose: A getter clears a field of a boundary box held in a local.
// exercises: shared-location-narrowing, boundary-box, getter
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the boxed local field read
function make(): SGPUProbeColorTargetState | null { return new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3); }
class K {
  t: SGPUProbeColorTargetState | null;
  constructor(t: SGPUProbeColorTargetState | null) { this.t = t; }
  get acc(): i32 { const p = this.t; if (p !== null) { p.blend = null; } return 0; }
}
export function main(): void {
  const t: SGPUProbeColorTargetState | null = make();
  const t2 = t;
  const k = new K(t2);
  if (t !== null && t2 !== null) {
    if (t.blend !== null) {
      const { acc } = k;
      print(`${t.blend.colorOperation}`);
    }
  }
}
