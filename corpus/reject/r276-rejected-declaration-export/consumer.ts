// corpus: reject/r276-rejected-declaration-export/consumer
// purpose: Follows poisoned names through a second export edge.
// exercises: module-import
// questions: compiler.md §128

import { first, second } from "./main";
