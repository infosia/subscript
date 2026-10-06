// corpus: reject/r379-async-arrow-capture
// purpose: An async arrow cannot retain a local capture.
// exercises: async-arrow, capture
// questions: compiler.md §167 rule 5, collisions.md C24 row 36
// tsc: accepts
// expected-error: S009, the async arrow captures the local n

export async function main(): Promise<void> {
  const n: i32 = 7;
  const job = async (): Promise<i32> => n;
  print(`${await job()}`);
}
