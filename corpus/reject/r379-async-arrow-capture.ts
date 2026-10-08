// corpus: reject/r379-async-arrow-capture
// purpose: An async arrow cannot retain a mutable local capture.
// exercises: async-arrow, capture
// questions: compiler.md §181 rule 1, collisions.md C24 row 36
// tsc: accepts
// expected-error: S009, the async arrow captures the local n

export async function main(): Promise<void> {
  let n: i32 = 7;
  const job = async (): Promise<i32> => n;
  print(`${await job()}`);
}
