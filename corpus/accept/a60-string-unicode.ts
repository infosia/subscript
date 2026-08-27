// corpus: accept/a60-string-unicode
// purpose: Pins Q21 Unicode Default Case Conversion and ECMA WhiteSpace +
//          LineTerminator trimming while retaining Q5 UTF-8 byte views.
// exercises: string-methods, unicode-case, ecma-whitespace
// questions: Q5, Q21
// tsc: accepts; js-comparable: yes
export function main(): void {
  print(`upper-sharp ${"ß".toUpperCase()}`);
  print(`upper-ligature ${"ﬄ".toUpperCase()}`);
  print(`sigma-word ${"ΟΣ".toLowerCase()}`);
  print(`sigma-pair ${"ΣΣς".toUpperCase()} ${"ΣΣς".toLowerCase()}`);
  print(`dotted-lower [${"İ".toLowerCase()}]`);
  print(`dotless-upper ${"ı".toUpperCase()}`);
  // These mapped results are ASCII, so their Q5 byte lengths also match
  // Node's UTF-16 lengths while still pinning expansion allocation.
  print(`case-lengths ${"ß".toUpperCase().length} ${"ﬄ".toUpperCase().length}`);

  const padded: string =
    "\uFEFF\u00A0\u1680\u2000\u200A\u202F\u205F\u3000\u2028Unicode\u2029\u3000\u205F\u202F\u200A\u2000\u1680\u00A0\uFEFF";
  print(`trim [${padded.trim()}]`);
  print(`trim-start ${padded.trimStart().startsWith("Unicode")}`);
  print(`trim-end ${padded.trimEnd().endsWith("Unicode")}`);
  // U+0085 is Unicode White_Space but not in ECMA's trim set.
  print(`nel-kept ${"\u0085x\u0085".trim() === "\u0085x\u0085"}`);
}
