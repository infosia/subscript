// corpus: reject/r266-boxed-local-narrowing-alias-store
// purpose: Reject a boxed local field read after a shared narrowing ends.
// exercises: shared-location-narrowing, boundary-box
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 14
function make(): SGPUProbeColorTargetState | null { return new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3); }
export function main(): void {
  const t: SGPUProbeColorTargetState | null = make();
  const t2 = t;
  if (t !== null && t2 !== null) {
    if (t.blend !== null) {
      t2.blend = null;
      print(`${t.blend.colorOperation}`);
    }
  }
}
