// corpus: reject/r398-async-arrow-captures-let
// purpose: Mutable async captures require an explicit immutable copy or class field.
// exercises: async-arrow, mutable-capture
// questions: compiler.md §181 rule 1, collisions.md C24
// tsc: accepts
// expected-error: S009, copy it into a `const` first, or use a class with a field
export async function main(): Promise<void> {
  let n: i32 = 7;
  const job = async (): Promise<i32> => n;
  print(`${await job()}`);
}
