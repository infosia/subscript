// corpus: reject/r268-renamed-import-missing/main
// purpose: Reports the missing imported name at its source position.
// exercises: module-import
// questions: compiler.md §126
// tsc: rejects TS2305
// expected-error: S016 at line 8, the imported name missing

import { missing as present } from "./lib";
export function main(): void { print(`${present()}`); }
