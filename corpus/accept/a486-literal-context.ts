// corpus: accept/a486-literal-context
// purpose: Set type arguments and plain normalize templates give literal context.
// observable: Empty Set sizes, a string element, and NFC text print their results.
// exercises: Set, empty-array, type-argument, string-normalize, template-literal
// questions: compiler.md §336
// tsc: accepts; js-comparable: yes
export function main(): void {
  const numbers = new Set<i32>([]);
  print(`numbers:${numbers.size}`);
  const words = new Set<string>([]);
  words.add("word");
  print(`words:${words.size}:${words.has("word")}`);
  const text: string = "e\u0301";
  print(text.normalize(`NFC`));
}
