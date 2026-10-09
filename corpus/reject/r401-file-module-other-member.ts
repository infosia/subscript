// corpus: reject/r401-file-module-other-member
// purpose: The file module admits only readFile and writeFile.
// exercises: file-module, named-import
// questions: compiler.md §185 rule 1, collisions.md C25
// tsc: rejects TS2305
// js-comparable: no C25: The module rejects another member.
// expected-error: S100, readFile(path
// enable-module: node:fs/promises
import { stat } from "node:fs/promises";
export async function main(): Promise<void> {
  await stat("text.txt");
}

// pin: 4fd0a713
// pin-dev-jit: Rejected before execution; --enable-module is unknown, or the standard module is missing.
// pin-c-aot: Rejected before C emission; --enable-module is unknown, or the standard module is missing.
