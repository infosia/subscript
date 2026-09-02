// corpus: reject/r176-unknown-member
// purpose: Rejects an unknown method on a class receiver.
// exercises: unknown-member
// questions: §82.2
// tsc: rejects TS2339
// expected-error: S018 at the unknown member
class Store {}
export function main(): void {
  const s: Store = new Store();
  s.store(1);
}
