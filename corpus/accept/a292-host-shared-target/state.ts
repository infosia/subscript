// corpus: accept/a292-host-shared-target/state
// purpose: Shares one module state between aliases.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts; js-comparable: yes
let count: i32 = 0;
export function update(): void { count += 1; print(`${count}`); }
