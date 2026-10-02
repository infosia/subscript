// corpus: accept/a314-conditional-join
// purpose: A conditional joins nullable branch types in both orders and evaluates only its selected branch.
// exercises: conditional-expression, nullable-reference, nullable-function, branch-order, nested-conditional, flow-narrowing
// questions: compiler.md §146, collisions.md C7
// tsc: accepts; js-comparable: yes
class Box {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function box(label: string): Box { print(label); return new Box(7); }
function callable(label: string): () => i32 { print(label); return (): i32 => 9; }
function nullable(label: string, keep: boolean): Box | null {
  print(label);
  return keep ? new Box(8) : null;
}
function exercise(flag: boolean): void {
  print(`${flag}`);
  const a = flag ? box("a-box") : null;
  if (a !== null) { print(`${a.value}`); } else { print("null"); }
  const b = flag ? null : box("b-box");
  if (b !== null) { print(`${b.value}`); } else { print("null"); }
  const c = flag ? box("c-box") : nullable("c-nullable", flag);
  if (c !== null) { print(`${c.value}`); } else { print("null"); }
  const d = flag ? nullable("d-nullable", flag) : box("d-box");
  if (d !== null) { print(`${d.value}`); } else { print("null"); }
  const e = flag ? callable("e-function") : null;
  if (e !== null) { print(`${e()}`); } else { print("null"); }
  const f = flag ? null : callable("f-function");
  if (f !== null) { print(`${f()}`); } else { print("null"); }
  const g = flag ? (flag ? box("g-box") : null) : null;
  if (g !== null) { print(`${g.value}`); } else { print("null"); }
  const h = flag ? null : (flag ? null : box("h-box"));
  if (h !== null) { print(`${h.value}`); } else { print("null"); }
}
export function main(): void { exercise(true); exercise(false); }
