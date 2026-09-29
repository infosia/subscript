// corpus: accept/a293-module-only-update/main
// purpose: Accepts the retired r267 program with one host update.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts; js-comparable: yes
import "./lib";
export function update(): void {}
export function main(): void { update(); }
