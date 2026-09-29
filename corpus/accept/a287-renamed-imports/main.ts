// corpus: accept/a287-renamed-imports/main
// purpose: Renamed imports keep declaration identity and live global storage.
// exercises: module-import, declaration-identity
// questions: compiler.md §126
// tsc: accepts; js-comparable: yes

import { g as f, Box as Crate, count as total, bump as increment,
  Kind as Choice, Label as Status, pick as select, Holder as Container } from "./lib";
export function main(): void {
  print(`function=${f()}`);
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
