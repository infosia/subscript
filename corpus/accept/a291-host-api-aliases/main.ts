// corpus: accept/a291-host-api-aliases/main
// purpose: Exposes two module functions under distinct host names.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts; js-comparable: yes
import { update as physics } from "./physics";
import { update as audio } from "./audio";
export { update as physicsUpdate } from "./physics";
export { update as audioUpdate } from "./audio";
export function main(): void { physics(); audio(); }
