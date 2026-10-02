// corpus: accept/a320-namespace-shadow/main
// purpose: Local declarations, parameters, blocks, and lambdas shadow namespace imports.
// exercises: namespace-import, local-shadow, block-scope, lambda
// questions: compiler.md §148
// tsc: accepts; js-comparable: yes
import * as ns from "./lib";
class Local { count: string = "local"; }
function parameter(ns: Local): void { print(ns.count); }
function local(): void { const ns = new Local(); print(ns.count); }
export function main(): void {
  local();
  parameter(new Local());
  { const ns = new Local(); print(ns.count); }
  const lambda: (ns: Local) => string = (ns: Local): string => ns.count;
  print(lambda(new Local()));
  const captured: () => string = (): string => {
    const ns = new Local();
    return ns.count;
  };
  print(captured());
  print(`${ns.count}`);
}
