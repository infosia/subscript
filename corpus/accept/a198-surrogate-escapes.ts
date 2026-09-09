// corpus: accept/a198-surrogate-escapes
// purpose: Checks decoded surrogate pairs and the UTF-8 byte count under compiler section 96.
// exercises: string-escape, template-literal, string-equality, string-length
// questions: Q5
// tsc: accepts; js-comparable: no Q5: String lengths count UTF-8 bytes instead of UTF-16 units.
// Measured subscript / node lengths: pair and literal 5/3; template 6/4; adjacent 10/5.
// Start, middle, and end: 6/4 each; interpolated parts: 14/8; escaped backslash: 6/6; brace: 4/2.
// Both print true for equality, the same strings, and 1 for the identifier control.
export function main(): void {
  const pair: string = "\ud83d\udc4dZ";
  const literal: string = "👍Z";
  print(`${pair === literal}`);
  print(`${pair}|${pair.length}|${literal.length}`);
  const template: string = `t\ud83d\udc4du`;
  print(`${template}|${template.length}`);
  const adjacent: string = "\u00e9\ud83d\udc4d\u{1F600}";
  print(`${adjacent}|${adjacent.length}`);
  const start: string = "\ud83d\udc4dab";
  const middle: string = "a\ud83d\udc4db";
  const end: string = "ab\ud83d\udc4d";
  print(`${start}|${start.length}`);
  print(`${middle}|${middle.length}`);
  print(`${end}|${end.length}`);
  const parts: string = `\ud83d\udc4d${"x"}\ud83d\udc4d${"y"}\ud83d\udc4d`;
  print(`${parts}|${parts.length}`);
  const backslash: string = "\\ud83d";
  print(`${backslash}|${backslash.length}`);
  const brace: string = "\u{1F600}";
  print(`${brace}|${brace.length}`);
  const \u{10400} = 1;
  print(`${𐐀}`);
}
