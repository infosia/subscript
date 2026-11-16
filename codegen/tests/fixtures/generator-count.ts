function* gen(): Generator<i32> { yield 1; }
function pick(value: Generator<i32>): Generator<i32> { return value; }
export function main(): void {
  const value = gen();
  const copy = pick(value);
  print(`${copy.next().value}`);
}
