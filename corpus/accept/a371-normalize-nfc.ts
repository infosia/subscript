// corpus: accept/a371-normalize-nfc
// purpose: normalize() and normalize("NFC") give the NFC form; no other operation normalizes.
// exercises: normalize, string, equality, set
// questions: compiler.md §193 rule 3, compiler.md §193 rule 5
// tsc: accepts; js-comparable: yes
function check(label: string, s: string): void {
  const nfc: string = s.normalize();
  const explicit: string = s.normalize("NFC");
  print(`${label}: ${nfc} same=${nfc === explicit} unchanged=${nfc === s} idempotent=${nfc.normalize() === nfc}`);
}

export function main(): void {
  check("ascii", "Hello, world");
  check("japanese", "日本語のテキスト");
  check("supplementary kanji", "𠮷野家");
  check("combining dakuten", "か\u3099き\u3099");
  check("precomposed dakuten", "がぎ");
  check("zwj family", "\u{1F469}\u200D\u{1F469}\u200D\u{1F467}\u200D\u{1F466}");
  check("regional pair", "\u{1F1EF}\u{1F1F5}");
  // Canonical reordering: dot below (ccc 220) before dot above (ccc 230).
  check("reorder", "D\u0307\u0323");
  // A singleton: U+212B ANGSTROM SIGN becomes U+00C5.
  check("singleton", "\u212B");
  // Equality, search, and Set keys use the exact bytes.
  const decomposed: string = "か\u3099";
  const composed: string = "が";
  print(`${decomposed === composed} ${decomposed.normalize() === composed}`);
  print(`${"がか\u3099".indexOf(composed)} ${"がか\u3099".includes(decomposed)}`);
  const raw: Set<string> = new Set<string>();
  const keys: Set<string> = new Set<string>();
  for (const word of [decomposed, composed, "か\u3099"]) {
    raw.add(word);
    keys.add(word.normalize("NFC"));
  }
  print(`raw keys ${raw.size}, NFC keys ${keys.size}, has ${keys.has(composed)}`);
}
