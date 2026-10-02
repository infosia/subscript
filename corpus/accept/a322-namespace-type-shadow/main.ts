// corpus: accept/a322-namespace-type-shadow/main
// purpose: Qualified namespace types resolve through value bindings and type parameters.
// exercises: namespace-import, type-reference, parameter, for-of, local-shadow, type-parameter
// questions: compiler.md §148
// tsc: accepts; js-comparable: yes
import * as ns from "./lib";
import { C, mk } from "./lib";
function parameter(ns: i32): ns.C {
  const c: ns.C = mk(ns);
  return c;
}
function named(C: i32): C {
  const c: C = mk(C);
  return c;
}
function local(): void {
  const ns: ns.C = mk(7);
  const c: ns.C = ns;
  print(`${c.value}`);
}
function qualified<ns>(value: ns.C): ns.C { return value; }
function unused<ns>(value: ns.C): void {}
export function main(): void {
  print(`${parameter(3).value} ${named(3).value}`);
  const xs: ns.C[] = [mk(4), mk(5)];
  for (const ns of xs) {
    const c: ns.C = ns;
    print(`${c.value}`);
  }
  local();
  unused<i32>(mk(11));
  const c: ns.C = ns.mk(9);
  print(`${c.value}`);
  print(`${qualified<i32>(mk(11)).value}`);
}
