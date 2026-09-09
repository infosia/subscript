// corpus: accept/a198-surrogate-escapes
// purpose: Checks decoded surrogate pairs and the UTF-8 byte count under compiler section 96.
// exercises: string-escape, template-literal, string-equality, string-length
// questions: Q5
// tsc: accepts; js-comparable: no Q5: String lengths count UTF-8 bytes instead of UTF-16 units.
// Measured subscript / node lengths: pair and literal 5/3; template 6/4; adjacent 10/5.
// Start, middle, and end: 6/4 each; interpolated parts: 14/8; escaped backslash: 6/6; brace: 4/2.
// Mixed and brace pairs and LF continuations: 4/2; interpolated parts: 9/5.
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
  const fixedBrace: string = "\ud83d\u{dc4d}";
  print(`${fixedBrace}|${fixedBrace.length}|${fixedBrace === "👍"}`);
  const fixedBraceParts: string = `\ud83d\u{dc4d}${"x"}\ud83d\u{dc4d}`;
  print(`${fixedBraceParts}|${fixedBraceParts.length}`);
  const braceFixed: string = "\u{d83d}\udc4d";
  print(`${braceFixed}|${braceFixed.length}|${braceFixed === "👍"}`);
  const braceFixedParts: string = `\u{d83d}\udc4d${"x"}\u{d83d}\udc4d`;
  print(`${braceFixedParts}|${braceFixedParts.length}`);
  const braceBrace: string = "\u{d83d}\u{dc4d}";
  print(`${braceBrace}|${braceBrace.length}|${braceBrace === "👍"}`);
  const braceBraceParts: string = `\u{d83d}\u{dc4d}${"x"}\u{d83d}\u{dc4d}`;
  print(`${braceBraceParts}|${braceBraceParts.length}`);
  const continued: string = "\ud83d\
\udc4d";
  print(`${continued}|${continued.length}|${continued === "👍"}`);
  const continuedParts: string = `\ud83d\
\udc4d${"x"}\ud83d\
\udc4d`;
  print(`${continuedParts}|${continuedParts.length}`);
}
