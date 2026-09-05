// corpus: accept/a89-interop-chain-payload
// interpreter: no — calls the synthetic native interop library
// purpose: Reads an extension payload through its embedded chain header.
// exercises: interop-chain, embedded-header-address, payload-read, foreign-call
// questions: Q13, Q16, compiler.md §23.7a
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const extension: SubChainExtA = new SubChainExtA(
    new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null),
    7.75,
    5,
  );

  // The chain argument receives the address of the live embedded header.
  // A copied header has the same depth but does not expose these payloads.
  print(`${subChainPayloadValue(extension.header)}`);
}
