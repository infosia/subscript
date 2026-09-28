// corpus: accept/a277-boundary-value-copy-narrowing
// purpose: Real boundary value copies keep their local field narrowing.
// exercises: boundary-value-copy, local-narrowing
// questions: compiler.md §124, C2
// tsc: accepts; js-comparable: no C2: The interop mirror classes have no JavaScript shim for node.
function touch(): void {}
class H { t: SGPUProbeColorTargetState = new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3); }
const h: H = new H();
function clear(): void { h.t.blend = null; }
export function main(): void {
  const value = new SGPUProbeColorTargetState(1, new SGPUProbeBlendState(5, 6), 3);
  const copy = value;
  if (value.blend !== null) {
    copy.blend = null;
    print(`copy ${copy.blend === null}`);
    touch();
    print(`b1 ${value.blend.colorOperation}`);
  }
  const fieldCopy = h.t;
  if (fieldCopy.blend !== null) {
    clear();
    print(`b4 ${fieldCopy.blend.colorOperation}`);
  }
}
