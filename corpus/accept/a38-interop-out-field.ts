// corpus: accept/a38-interop-out-field
// interpreter: no — calls the synthetic native interop library
// purpose: Passes a boundary struct by reference; the callee WRITES its fields and the script reads them after the call.
// exercises: interop-out-field, boundary-struct-by-reference, callee-writes, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// §14.3 out field. The out/mutable case is spelled here as a caller-provided
// boundary struct passed by reference — the `Struct | null` boundary form.
// The callee WRITES the struct's fields; the script reads them back after
// the call. There is no copy-back: both tiers pass the ADDRESS of the
// language struct's own storage (layout-identical to the C struct,
// invariant 1), so the callee wrote the caller's storage directly. This is
// the out-field spelling of §14.3; the P7.2 out-array capstone fills many
// such per-future status records. subDeviceQuery writes future = request*10
// + chain depth (0 for a depth-0 device) and completed = 1.

export function main(): void {
  // Q16: self-created handle via subDeviceCreate(null) (chain depth 0).
  const device: SubDevice = subDeviceCreate(null);

  const status: SubQueryStatus = new SubQueryStatus(0, 0);
  subDeviceQuery(device, 7, status);

  print(`${status.future}`); // 70 — written by the callee
  print(`${status.completed}`); // 1 — written by the callee
}
