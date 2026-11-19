// corpus: accept/a492-file-node-text-bytes
// interpreter: no — calls the host file provider
// purpose: Text and byte reads agree with node in a scratch directory.
// exercises: file-module, async, byte-array
// questions: compiler.md §340 rule 1
// compiler.md §340 rule 1: debug dev + ship 565 ms; 1 C build.
// tsc: accepts
// js-comparable: yes
// enable-module: node:fs/promises
// file-provider: scratch
import { readFile, writeFile } from "node:fs/promises";
export async function main(): Promise<void> {
  await writeFile("text.txt", "Hi!");
  print(await readFile("text.txt", "utf8"));
  const bytes: u8[] = await readFile("text.txt");
  print(`${bytes.length}`);
  print(`${bytes[0]} ${bytes[1]} ${bytes[2]}`);
  await writeFile("copy.txt", bytes);
  print(await readFile("copy.txt", "utf8"));
}
