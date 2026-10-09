// corpus: reject/r399-file-module-disabled
// purpose: A build must enable the host file module.
// exercises: file-module, build-option
// questions: compiler.md §185 rule 2, collisions.md C25
// tsc: accepts
// js-comparable: no C25: The host must enable the module at build time.
// expected-error: S100, --enable-module node:fs/promises
import { readFile } from "node:fs/promises";
export async function main(): Promise<void> {
  print(await readFile("text.txt", "utf8"));
}

// pin: 4fd0a713
// pin-dev-jit: Rejected before execution; --enable-module is unknown, or the standard module is missing.
// pin-c-aot: Rejected before C emission; --enable-module is unknown, or the standard module is missing.
