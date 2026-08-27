// corpus: accept/a107-interop-handle-parameter-pair
// purpose: Collapses an adjacent const opaque-handle parameter pair beside a leading handle, preserving count and per-element identity in C.
// exercises: handle-parameter-pair, input-array, count-elision, opaque-handle, pointer-identity, foreign-call
// questions: Q13, C4
// tsc: accepts
// compiler.md §34. Repeating both the leading queue handle and the first
// command distinguishes queue identity from within-array identity without
// relying on process-global fixture state.

export function main(): void {
  const queue: SubDevice = subDeviceCreate(null);
  const first: SubDevice = subDeviceCreate(null);
  const second: SubDevice = subDeviceCreate(null);
  const commands: SubDevice[] = [first, queue, second, first, queue];

  let selector: u32 = 0;
  while (selector <= 5) {
    print(`${subProbeQueueSubmitCheck(queue, commands, selector)}`);
    selector = selector + 1;
  }

  subDeviceRelease(queue);
  subDeviceRelease(first);
  subDeviceRelease(second);
}
