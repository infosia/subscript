// corpus: reject/r178-optional-chain-unbound
// purpose: Rejects an optional chain whose undefined result will bind in TypeScript.
// exercises: optional-chain, undefined
// questions: §82.3, C7
// tsc: accepts
// expected-error: S012 at the optional chain
class Box {
  v: i32 = 1;
}
export function main(): void {
  const x: Box | null = new Box();
  print(`${x?.v}`);
}
