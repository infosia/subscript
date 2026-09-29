// corpus: accept/a288-re-export-kinds/main
// purpose: Reads each re-export kind and observes a write to the original global.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
export function main(): void {
  print(`function=${call()}`);
  const box: Crate = new Crate();
  print(`class=${box.value}`);
  print(`before=${total}`);
  increment();
  print(`after=${total}`);
  print(`enum=${Choice.Value}`);
  const label: Status = "ready";
  print(`alias=${label}`);
  print(`generic-function=${select<i32>(7)}`);
  const holder: Container<i32> = new Container<i32>(8);
  print(`generic-class=${holder.value}`);
}
