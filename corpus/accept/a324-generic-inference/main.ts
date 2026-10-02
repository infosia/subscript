// corpus: accept/a324-generic-inference/main
// purpose: Infers generic function arguments before instantiation.
// exercises: generic-function, namespace-import, async-function
// questions: compiler.md §149
// tsc: accepts; js-comparable: yes
import * as ns from "./lib";
import { id as importedId } from "./lib";
function id<T>(x: T): T { return x; }
function pair<T>(a: T, b: T): T { return a; }
function first<T>(xs: T[]): T { return xs[0]; }
class Box { value: i32 = 9; }
function orNull<T>(x: T | null, d: T): T { return x === null ? d : x; }
function second<A, B>(a: A, b: B): B { return b; }
function nested<T>(xs: Map<string, T[]>): i32 { return xs.size; }
async function asyncFirst<T>(xs: T[]): Promise<T> { return xs[0]; }
function witness(label: string, value: i64): i64 { print(label); return value; }
export async function main(): Promise<void> {
  const n: i64 = 12;
  print(`${id(n)} ${id<i64>(n)}`);
  print(`${id(7)} ${id<i32>(7)}`);
  print(`${pair(n, 1)} ${pair<i64>(n, 1)}`);
  print(`${pair(1, n)} ${pair<i64>(1, n)}`);
  const small: i32 = 4;
  const fraction: f64 = 2.5;
  // The conflicting Problem-table call has its reject witness in r330.
  print(`${pair(small as f64, fraction)} ${pair<f64>(small as f64, fraction)}`);
  const xs: u8[] = [6];
  print(`${first(xs)} ${first<u8>(xs)}`);
  const b: Box | null = null;
  print(`${orNull(b, new Box()).value} ${orNull<Box>(b, new Box()).value}`);
  print(`${ns.id(n)} ${ns.id<i64>(n)}`);
  print(`${importedId(n)} ${importedId<i64>(n)}`);
  print(`${second(n, "text")} ${second<i64, string>(n, "text")}`);
  const map: Map<string, u8[]> = new Map<string, u8[]>();
  map.set("key", xs);
  print(`${nested(map)} ${nested<u8>(map)}`);
  const a: u8 = await asyncFirst(xs);
  const e: u8 = await asyncFirst<u8>(xs);
  print(`${a} ${e}`);
  print(`${id(2.5)} ${id<f64>(2.5)}`);
  print(`${id("text")} ${id<string>("text")}`);
  print(`${first([7])} ${first<i32>([7])}`);
  print(`${pair(n, 3000000000)} ${pair<i64>(n, 3000000000)}`);
  print(`${pair(witness("left", 21), witness("right", 22))}`);
  print(`${pair<i64>(witness("left", 21), witness("right", 22))}`);
}
