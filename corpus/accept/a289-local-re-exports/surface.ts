// corpus: accept/a289-local-re-exports/surface
// purpose: Exports a local declaration and a renamed import under new names.
// exercises: module-export, module-import
// questions: compiler.md §128
// tsc: accepts; js-comparable: yes

import { value as imported } from "./lib";
function local(): i32 { return imported() + 3; }
export { local as other, imported as again };
