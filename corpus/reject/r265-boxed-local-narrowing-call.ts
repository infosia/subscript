// corpus: reject/r265-boxed-local-narrowing-call
// purpose: Reject a boxed local field read after a shared narrowing ends.
// exercises: shared-location-narrowing, boundary-box
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class H { t: SGPUProbeColorTargetState | null = new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3); }
const h: H = new H();
function clear(): void { const p = h.t; if (p !== null) { p.blend = null; } }
export function main(): void {
  const t = h.t;
  if (t !== null) {
    if (t.blend !== null) {
      clear();
      print(`${t.blend.colorOperation}`);
    }
  }
}
