// corpus: accept/a292-host-shared-target/main
// purpose: Exposes one implementation with two host names.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts; js-comparable: yes
import { update } from "./state";
export { update as first, update as second } from "./state";
export function main(): void { update(); update(); }
