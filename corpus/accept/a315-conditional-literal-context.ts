// corpus: accept/a315-conditional-literal-context
// purpose: Conditional literal branches adopt the other branch type in both orders.
// exercises: conditional-expression, contextual-literal, sized-numerics, string-literal-union
// questions: compiler.md §146, collisions.md C4, Q32
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";
function exercise(flag: boolean, n: i64, f: f64, u: u8, mode: Mode): void {
  print(`${flag ? n : 0}`);
  print(`${flag ? 0 : n}`);
  print(`${flag ? f : 0}`);
  print(`${flag ? 0 : f}`);
  print(`${flag ? u : 0}`);
  print(`${flag ? 0 : u}`);
  print(`${flag ? mode : "a"}`);
  print(`${flag ? "a" : mode}`);
  print(`${flag ? n : (-1)}`);
  print(`${flag ? (-1) : n}`);
}
export function main(): void {
  exercise(true, 42, 2.5, 7, "b");
  exercise(false, 42, 2.5, 7, "b");
}
