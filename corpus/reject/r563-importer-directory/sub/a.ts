// corpus: reject/r563-importer-directory/sub/a
// purpose: A relative specifier resolves against the directory of its importer, so ./x names sub/x, which is absent.
// exercises: module-import
// questions: compiler.md §354 rule 2
// tsc: rejects TS2307
// expected-error: S100 at line 7, imported module `./x` is not among the program's files
import { x } from "./x";
export function fromSub(): i32 { return x; }
