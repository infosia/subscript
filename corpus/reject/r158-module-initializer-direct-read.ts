// corpus: reject/r158-module-initializer-direct-read
// purpose: Rejects a module initializer that reads a later data binding directly.
// exercises: module-initializer, declaration-order, direct-global-read
// questions: §67
// tsc: rejects TS2448, TS2454
// expected-error: S100 at the initializer read

const first: i32 = second;
const second: i32 = 2;

export function main(): void {
  print(`${first}:${second}`);
}
