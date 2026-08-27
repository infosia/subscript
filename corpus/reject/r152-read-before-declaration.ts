// corpus: reject/r152-read-before-declaration
// purpose: Rejects a nested lambda read before the declaration in its block.
// exercises: block-scope, read-before-declaration, nested-lambda
// questions: §67
// tsc: accepts
// expected-error: S100 at the read in the nested lambda

export function main(): void {
  const value: i32 = 3;
  {
    const read: () => i32 = (): i32 => value;
    const value: i32 = 4;
    print(`${read()}`);
  }
}
