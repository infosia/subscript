// corpus: accept/a305-generic-json-load-dump
// purpose: Accepts generic JSON load and dump at admitted instances.
// exercises: generic-function, generic-class, json
// questions: compiler.md §143, stdlib.md §13
// tsc: accepts; js-comparable: yes
class Pt { x: i32 = 0; y: i32 = 0; }
function load<T>(text: string): T { return JSON.parse<T>(text); }
function dump<T>(value: T): string { return JSON.stringify(value); }
class Repo<T> { load(text: string): T { return JSON.parse<T>(text); } }
export function main(): void {
  const p = load<Pt>("{\"x\":1,\"y\":2}");
  print(dump<Pt>(p));
  print(dump<i32>(3));
  const points = load<Pt[]>("[{\"x\":4,\"y\":5}]");
  print(`${points[0].x + points[0].y}`);
  const repo = new Repo<Pt>();
  const q = repo.load("{\"x\":6,\"y\":7}");
  print(`${q.x + q.y}`);
}
