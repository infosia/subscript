// corpus: accept/a457-local-lambdas-in-a-group-share-one-environment
// purpose: Local lambdas in a group share one environment.
// observable: Group calls and values retain captures across recursion and escape.
// exercises: lambda, recursion, capture, async-arrow, owned-environment
// questions: compiler.md §293, §284, §274
// tsc: accepts; js-comparable: yes
const log: string[] = [];
class Box { value: i32; constructor(value: i32) { this.value = value; } }
function parity(): void {
  const label: string = "parity";
  const isEven = (n: i32): boolean => { if (n == 0) { log.push(label); return true; } return isOdd(n - 1); };
  const isOdd = (n: i32): boolean => { if (n == 0) return false; return isEven(n - 1); };
  log.push(`${isEven(4)}:${isOdd(3)}`);
}
function table(): void {
  const first = (n: i32): void => { log.push(`f${n}`); if (n > 0) second(n - 1); };
  const second = (n: i32): void => { first(n); };
  first(1);
  const local = (n: i32): void => { log.push(`l${n}`); if (n > 0) { const other = local; other(n - 1); } };
  local(1);
}
function stored(): void {
  const values: ((n: i32) => i32)[] = [];
  const base: i32 = 10;
  const first = (n: i32): i32 => { values.push(second); return base + n; };
  const second = (n: i32): i32 => base + n + 1;
  log.push(`${first(2)}:${values[0](2)}:${second(3)}`);
}
function make(): (n: i32) => string {
  const text: string = "kept";
  const box: Box = new Box(7);
  const first = (n: i32): string => { if (n > 0) return second(n - 1); return `${text}:${box.value}`; };
  const second = (n: i32): string => { const value = first; return value(n); };
  return second;
}
function triple(): void {
  const offset: i32 = 20;
  const first = (n: i32): i32 => { if (n == 0) return offset; return second(n - 1); };
  const second = (n: i32): i32 => third(n);
  const third = (n: i32): i32 => first(n);
  log.push(`${first(3)}`);
}
export async function main(): Promise<void> {
  parity(); table(); stored();
  const value = make();
  log.push(value(3));
  triple();
  const prefix: string = "async";
  const first = async (n: i32): Promise<void> => { log.push(`${prefix}${n}`); if (n > 0) await second(n - 1); };
  const second = async (n: i32): Promise<void> => { await first(n); };
  await first(2);
  const captured: i32 = 30;
  const withDefault = (n: i32 = captured): i32 => partner(n);
  const partner = (n: i32): i32 => n + 1;
  log.push(`${withDefault()}`);
  print(log.join(" "));
}
