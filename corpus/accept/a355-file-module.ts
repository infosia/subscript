// corpus: accept/a355-file-module
// interpreter: no — calls the host file provider
// purpose: Read and write whole files through a host provider.
// exercises: file-module, host-completion, string, byte-array, collect, error
// questions: compiler.md §185 rules 1–8, collisions.md C25
// tsc: accepts
// js-comparable: no C25: Host file errors and byte arrays differ from node.
// enable-module: node:fs/promises
// file-provider: scratch

import { readFile, writeFile } from "node:fs/promises";

export async function main(): Promise<void> {
  await writeFile("text.txt", "hello");
  print(await readFile("text.txt", "utf8"));
  const data: u8[] = [0, 128, 255];
  await writeFile("bytes.bin", data);
  const bytes = await readFile("bytes.bin");
  print(`${bytes.length}:${bytes[0]}:${bytes[1]}:${bytes[2]}`);
  try {
    await readFile("missing.txt", "utf8");
  } catch (error) {
    if (error instanceof Error) { print(`${error.name}:${error.message}`); }
  }
  await writeFile("pending.txt", "pending");
  const pending = readFile("pending.txt", "utf8");
  Context.collect();
  // The test provider defers this read until the next write callback.
  await writeFile("release.txt", "release");
  print(await pending);
}

// pin: 4fd0a713
// pin-dev-jit: Rejected before execution; --enable-module is unknown, or the standard module is missing.
// pin-c-aot: Rejected before C emission; --enable-module is unknown, or the standard module is missing.
