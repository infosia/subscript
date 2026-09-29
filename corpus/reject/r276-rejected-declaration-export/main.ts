// corpus: reject/r276-rejected-declaration-export/main
// purpose: Imports both rejected names directly and through a re-export.
// exercises: module-import, module-export
// questions: compiler.md §128

import "./consumer";
import { a, b } from "./lib";
export { a as first, b as second } from "./lib";
