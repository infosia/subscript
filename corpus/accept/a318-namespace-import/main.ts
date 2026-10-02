// corpus: accept/a318-namespace-import/main
// purpose: Namespace members and named imports resolve to the same declarations.
// exercises: namespace-import, module-export, live-import, generic-class, re-export
// questions: Q1, Q12
// tsc: accepts; js-comparable: yes
import * as ns from "./lib";
import * as chain from "./chain";
import { add, bump, count, C, G, E, A, id } from "./lib";
export function main(): void {
  print(`${ns.add(2, 3)} ${add(2, 3)}`);
  ns.bump();
  print(`${ns.count} ${count}`);
  bump();
  print(`${ns.count} ${count}`);
  const c: ns.C = new ns.C();
  const d: C = new C();
  print(`${c.value} ${d.value}`);
  const g: ns.G<i32> = new ns.G<i32>(8);
  const h: G<i32> = new G<i32>(8);
  print(`${g.value} ${h.value}`);
  print(`${ns.E.M} ${E.M}`);
  const a: ns.A = "ok";
  const b: A = "ok";
  print(`${a} ${b}`);
  print(`${ns.id<i32>(6)} ${id<i32>(6)}`);
  print(`${chain.sum(3, 4)} ${add(3, 4)}`);
}
