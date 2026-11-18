// corpus: reject/r547-capture-of-an-index-copy
// purpose: A capture of an array element names the array that can drop it.
// exercises: capture-block, array-element
// questions: compiler.md §331, C5
// tsc: accepts
// expected-error: S009: captured binding `h` borrows an element of array `hs`; the array can drop that element before the lambda runs; local `f` is declared outside that block

async function work(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> {
  const hs: Promise<i32>[] = [work(1), work(2)];
  let f: () => Promise<i32> = () => work(0);
  { const h = hs[1]; f = () => h; }
  print(`${await f()}`);
}
