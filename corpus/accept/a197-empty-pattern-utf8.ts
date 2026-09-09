// corpus: accept/a197-empty-pattern-utf8
// purpose: Checks compiler section 95 empty-pattern-utf8 semantics.
// exercises: string-methods, q14-formatting
// questions: Q5
// tsc: accepts; js-comparable: no Q5: UTF-8 code points differ from UTF-16 units.
// Node measurements: aé splits to ["a","é"]; replaceAll gives "-a-é-".
// Node: 😀a splits to ["\ud83d","\ude00","a"]; replaceAll gives "-\ud83d-\ude00-a-".
// subscript: 😀a splits to ["😀","a"]; replaceAll gives "-😀-a-".
// Node: 😀 splits to ["\ud83d","\ude00"]; replaceAll gives "-\ud83d-\ude00-".
// subscript: 😀 splits to ["😀"]; replaceAll gives "-😀-".
// subscript: aé matches Node.
export function main(): void {
  print(JSON.stringify("aé".split("")));
  print("aé".replaceAll("", "-"));
  print(JSON.stringify("😀a".split("")));
  print("😀a".replaceAll("", "-"));
  print(JSON.stringify("😀".split("")));
  print("😀".replaceAll("", "-"));
}
