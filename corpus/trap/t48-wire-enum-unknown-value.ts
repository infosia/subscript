// corpus: trap/t48-wire-enum-unknown-value
// purpose: An unknown C wire value traps while crossing into a mapped alias.
// exercises: CEnum, foreign-return, unknown-wire-value, trap-stop
// questions: R23, C6
// tier-policy: both tiers trap with kind 24
// expected-trap: wire-enum-unknown-value for SubWireMode value 12345 at the foreign call

export function main(): void {
  print("before");
  const value: SubWireMode = subWireModeUnknown();
  print(`${value}`);
}
