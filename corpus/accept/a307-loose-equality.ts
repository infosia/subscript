// corpus: accept/a307-loose-equality
// purpose: Accepts loose equality with strict equality rules.
// exercises: equality, inequality, enum, class, nullable, generic-function
// questions: compiler.md §144
// tsc: accepts; js-comparable: yes
class Box { value: i32 = 1; }
enum Kind { First, Second }
function same<T>(left: T, right: T): boolean { return left == right; }
function different<T>(left: T, right: T): boolean { return left != right; }
function absent(value: Box | null): boolean { return value == null; }
function present(value: Box | null): boolean { return value != null; }
function booleanDifferent(left: boolean, right: boolean): boolean { return left != right; }
function enumDifferent(left: Kind, right: Kind): boolean { return left != right; }
export function main(): void {
  const a: i32 = 3;
  const b: i32 = 4;
  print(`${a == a} ${a != b}`);
  const x: f64 = 1.5;
  const y: f64 = 2.5;
  print(`${x == x} ${x != y}`);
  const text: string = "same";
  const other: string = "other";
  print(`${text == text} ${text != other}`);
  const yes: boolean = true;
  const no: boolean = false;
  print(`${yes == yes} ${booleanDifferent(yes, no)}`);
  const first: Kind = Kind.First;
  const second: Kind = Kind.Second;
  print(`${first == first} ${enumDifferent(first, second)}`);
  const box = new Box();
  const alias = box;
  const distinct = new Box();
  print(`${box == alias} ${box != distinct}`);
  print(`${absent(null)} ${present(box)}`);
  print(`${same<i32>(a, a)} ${different<i32>(a, b)}`);
  print(`${same<Box>(box, alias)} ${different<Box>(box, distinct)}`);
}
