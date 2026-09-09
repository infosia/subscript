// corpus: accept/a195-empty-pattern
// purpose: Checks compiler section 95 empty-pattern semantics.
// exercises: string-methods, q14-formatting
// questions: Q21
// tsc: accepts; js-comparable: yes
export function main(): void {
  print(JSON.stringify("".split("")));
  print("".replaceAll("", "-"));
  print("".replace("", "-"));
  print(JSON.stringify("a".split("")));
  print("a".replaceAll("", "-"));
  print("a".replace("", "-"));
  print(JSON.stringify("ab".split("")));
  print("ab".replaceAll("", "-"));
  print("ab".replace("", "-"));
  print(JSON.stringify("a,b".split(",")));
  print("ab".replaceAll("b", "-"));
}
