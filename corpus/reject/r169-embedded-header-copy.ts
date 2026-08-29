// corpus: reject/r169-embedded-header-copy
// purpose: Rejects a chain header copied out of its enclosing boundary extension.
// exercises: embedded-header, intrusive-chain, value-copy
// questions: §33.5 rule 10
// tsc: accepts
// expected-error: S100 at the embedded header read
export function main(): void {
  const extension: SubChainExtA = new SubChainExtA(
    new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null),
    7.75,
    5,
  );
  const copied: SubChainHeader = extension.header;
  print(`${copied.sType}`);
}
