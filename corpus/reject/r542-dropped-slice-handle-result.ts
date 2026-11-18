// corpus: reject/r542-dropped-slice-handle-result
// purpose: A dropped slice result keeps the receiver origin unhandled.
// exercises: async-function, await, array, held-handle
// questions: compiler.md §329 rule 5, collisions.md C8
// tsc: accepts
// expected-error: S013 at line 10, column 15
async function work(): Promise<i32> { return 1; }
async function many(): Promise<Promise<i32>[]> { return [work()]; }
export async function main(): Promise<void> {
  const ps = (await many()).slice(0);
  print(`${ps.length}`);
}
