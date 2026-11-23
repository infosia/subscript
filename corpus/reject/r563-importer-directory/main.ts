// corpus: reject/r563-importer-directory/main
// purpose: Imports a module in a subdirectory and the module x beside it.
// exercises: module-import
// questions: compiler.md §354 rule 2
import { fromSub } from "./sub/a";
import { x } from "./x";
export function main(): void { print(`${fromSub()} ${x}`); }
