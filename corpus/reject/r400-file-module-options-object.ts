// corpus: reject/r400-file-module-options-object
// purpose: A file call accepts only the four contracted forms.
// exercises: file-module, options-object
// questions: compiler.md §185 rule 1, collisions.md C25
// tsc: rejects TS2345
// js-comparable: no C25: The module rejects an options-object argument.
// expected-error: S100, readFile(path
// enable-module: node:fs/promises
import { readFile } from "node:fs/promises";
export async function main(): Promise<void> {
  print(await readFile("text.txt", { encoding: "utf8" }));
}

// pin: 4fd0a713
// pin-dev-jit: Rejected before execution; --enable-module is unknown, or the standard module is missing.
// pin-c-aot: Rejected before C emission; --enable-module is unknown, or the standard module is missing.
