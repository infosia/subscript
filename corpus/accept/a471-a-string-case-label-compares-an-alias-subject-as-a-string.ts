// corpus: accept/a471-a-string-case-label-compares-an-alias-subject-as-a-string
// purpose: String labels compare alias subjects as strings (§321).
// observable: Matches, nullable labels, and source evaluation order.
// exercises: string-alias, switch, string
// questions: compiler.md §321
// tsc: accepts; js-comparable: yes
type Mode = "aaa" | "bbb";
function f(m: Mode, c: string): void {
  switch (m) { case c: print("hit"); break; default: print("miss"); }
}
function subject(): Mode { print("subject"); return "bbb"; }
function label(s: string): string { print(s); return s; }
function optional(s: string | null): string | undefined {
  if (s === null) { return undefined; }
  return s;
}
function nullable(m: Mode, c: string | null, u: string | null): void {
  switch (m) { case c: print("null hit"); break; case optional(u): print("undefined hit"); break; default: print("absent"); }
}
export function main(): void {
  f("aaa", "aaa"); f("bbb", "aaa"); f("aaa", "zzz");
  switch (subject()) {
    case "aaa": print("literal"); break;
    case label("zzz"): print("wrong"); break;
    case label("bbb"): print("mixed"); break;
    case label("late"): print("wrong"); break;
    default: print("miss");
  }
  nullable("aaa", "aaa", null);
  nullable("bbb", null, "bbb");
  nullable("aaa", null, null);
}
