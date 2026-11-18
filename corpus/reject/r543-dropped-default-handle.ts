// corpus: reject/r543-dropped-default-handle
// purpose: A default makes an origin that the body must discharge.
// exercises: async-function, default-parameter, held-handle
// questions: compiler.md §329 rule 3, collisions.md C8
// tsc: accepts
// expected-error: S013 at line 8, column 39
async function work(): Promise<i32> { return 1; }
async function drop(p: Promise<i32> = work()): Promise<i32> { return 2; }
export async function main(): Promise<void> { print(`${await drop()}`); }
