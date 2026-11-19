// corpus: accept/a489-file-literal-path
// interpreter: no — calls the host file provider
// purpose: Literal encodings and alias paths reach the host as strings.
// exercises: file-module, literal-alias, const, template
// questions: compiler.md §339 rules 1, 2, 4
// compiler.md §339: debug dev + ship: 0.404 s; 1 C build.
// tsc: accepts
// js-comparable: no C25: The host file provider uses a scratch directory.
// enable-module: node:fs/promises
// file-provider: scratch
import { readFile, writeFile } from "node:fs/promises";
type Name = "a.txt" | "b.txt";
const encoding = "utf8";
export async function main(): Promise<void> {
  const a: Name = "a.txt";
  const b: Name = "b.txt";
  const localEncoding = encoding;
  await writeFile(a, "alpha");
  await writeFile(b, "beta");
  print(await readFile(a, localEncoding));
  print(await readFile(b, `utf8`));
}
