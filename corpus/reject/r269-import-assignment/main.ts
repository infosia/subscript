// corpus: reject/r269-import-assignment/main
// purpose: Rejects a write to an imported mutable global.
// exercises: module-import, assignment
// questions: compiler.md §127
// tsc: rejects TS2632
// expected-error: S100 at line 10, count is an import

import { count, bump } from "./lib";
export function main(): void {
    count = 10;
    bump();
    print(`${count}`);
}
