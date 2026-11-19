// corpus: accept/a488-literal-const-context/main
// purpose: Literal const bindings and plain templates keep a literal place.
// exercises: literal-alias, const, template, import, return
// questions: compiler.md §339 rules 1–3
// compiler.md §339: debug dev + ship: 0.435 s; 1 C build.
// tsc: accepts
// js-comparable: yes
import { imported } from "./lib";
type Name = "a.txt" | "b.txt";
const moduleName = "a.txt";
function show(n: Name): void { print(n); }
function name(): Name { return moduleName; }
function template(): Name { return `b.txt`; }
function localName(): Name {
  const a = "a.txt"; const b = a; const c = b;
  return c;
}
function importedName(): Name { return imported; }
export function main(): void {
  const a = "a.txt";
  const b = a;
  const c = b;
  const local: Name = c;
  const moduleValue: Name = moduleName;
  const importedValue: Name = imported;
  const plain: Name = `b.txt`;
  show(local); show(moduleValue); show(importedValue); show(plain);
  show(a); show(moduleName); show(imported); show(`a.txt`);
  show(name()); show(template());
  show(localName()); show(importedName()); show(c);
}
