// corpus: accept/a286-mirror-type-alias-shadow/main
// purpose: A module class hides a mirror alias only in its own scope.
// exercises: module-import, declaration-identity, mirror-type-alias
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

import { libRead } from "./lib";
class SubLogCallback { value: i32 = 7; }
export function main(): void {
  const local: SubLogCallback = new SubLogCallback();
  print(`main=${local.value} lib=${libRead()}`);
}
