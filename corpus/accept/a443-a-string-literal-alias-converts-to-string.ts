// corpus: accept/a443-a-string-literal-alias-converts-to-string
// purpose: An alias value converts at a string place.
// observable: Each place prints the member text.
// exercises: string-alias, string, assignment, lambda, array, class
// questions: compiler.md §259
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";
type Wire = CEnum<{ low: 3; high: 7; }>;
class P { constructor(public s: string) {} }
function show(s: string): void { print(s); }
function memberName(x: Mode): string { return x; }
function optional(x: Mode): string | undefined { return x; }
function wireName(x: Wire): string { return x; }
function wireOptional(x: Wire): string | undefined { return x; }
export function main(): void {
  const m: Mode = "b";
  const w: Wire = "high";
  const s: string = m;
  print(s);
  let t: string = "x"; t = m; print(t);
  show(m); print(m);
  const p = new P(m); print(p.s);
  const xs: string[] = []; xs.push(m); print(xs[0]);
  print(memberName(m));
  const f = (): string => m; print(f());
  const ys: string[] = [m, "c"]; print(ys[0]);
  p.s = m; print(p.s);
  const n: string | null = m; print(`${n}`);
  const u = optional(m); if (u !== undefined) { print(u); }
  const wire: string = w; print(wire);
  print(`${s.length}`);
  wireForms();
}
function wireForms(): void {
  const m: Wire = "high";
  const w: Wire = "high";
  const s: string = m;
  print(s);
  let t: string = "x"; t = m; print(t);
  show(m); print(m);
  const p = new P(m); print(p.s);
  const xs: string[] = []; xs.push(m); print(xs[0]);
  print(wireName(m));
  const f = (): string => m; print(f());
  const ys: string[] = [m, "c"]; print(ys[0]);
  p.s = m; print(p.s);
  const n: string | null = m; print(`${n}`);
  const u = wireOptional(m); if (u !== undefined) { print(u); }
  const wire: string = w; print(wire);
  print(`${s.length}`);
}
