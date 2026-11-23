// corpus: accept/a510-module-paths/a/inner
// purpose: Imports the module beside it and the module in a sibling directory, and re-exports through a path.
// exercises: module-import, module-export, re-export, module-initializer
// questions: compiler.md §354 rule 2
import { label, count } from "./util";
import { label as otherLabel } from "../b/util";
export { Kind as OtherKind } from "../b/util";
export function inner(): string { return `${label()}${count} ${otherLabel()}`; }
print(`a/inner init ${label()} ${otherLabel()}`);
