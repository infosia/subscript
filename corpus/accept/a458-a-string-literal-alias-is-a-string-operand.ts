// corpus: accept/a458-a-string-literal-alias-is-a-string-operand
// purpose: Plain and wire aliases are string operands at the places of §299.
// observable: Each operation prints its result, with alias comparison and array controls.
// exercises: string-alias, string, operator, member, assertion, switch, array
// questions: compiler.md §299, §259, Q32
// tsc: accepts; js-comparable: yes
type Mode = "a" | "bb";
type Wire = CEnum<{ low: 3; high: 7; }>;
export function main(): void {
  plain("bb", "bb", "bb");
  wire("high", "high", "high");
}
function plain(m: Mode, m2: Mode, s: string): void {
  const xs = [m, "c"]; print(xs[0]); print(xs[1]);
  print(m as string);
  print(`${m === s}`); print(`${s !== m}`);
  print(`${m.length}`); print(m.toUpperCase());
  print("x" + m); print(m + "x"); print(m + m);
  let acc: string = "seed"; acc += m; print(acc);
  switch (s) { case m: print("match"); break; default: print("miss"); }
  print(`${m === "a"}`); print(`${m === m2}`);
  const c = "a"; print(`${m === c}`);
  const ys = [m, m2]; print(ys[0]); print(ys[1]);
  const zs = [m, "a"]; print(zs[0]); print(zs[1]);
  const ws = ["a", m]; print(ws[0]); print(ws[1]);
}
function wire(m: Wire, m2: Wire, s: string): void {
  const xs = [m, "c"]; print(xs[0]); print(xs[1]);
  print(m as string);
  print(`${m === s}`); print(`${s !== m}`);
  print(`${m.length}`); print(m.toUpperCase());
  print("x" + m); print(m + "x"); print(m + m);
  let acc: string = "seed"; acc += m; print(acc);
  switch (s) { case m: print("match"); break; default: print("miss"); }
  print(`${m === "low"}`); print(`${m === m2}`);
  const c = "low"; print(`${m === c}`);
  const ys = [m, m2]; print(ys[0]); print(ys[1]);
  const zs = [m, "low"]; print(zs[0]); print(zs[1]);
  const ws = ["low", m]; print(ws[0]); print(ws[1]);
}
