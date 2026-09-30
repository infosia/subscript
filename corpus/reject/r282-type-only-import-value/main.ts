// corpus: reject/r282-type-only-import-value/main
// purpose: Rejects a value use of a name bound by a type-only import.
// exercises: module-import
// questions: compiler.md §134
// tsc: rejects TS1361
// expected-error: S100 at line 10, Box was imported with import type

import type { Box } from "./lib";
export function main(): void {
    const b = new Box();
    print(`${b.value}`);
}
