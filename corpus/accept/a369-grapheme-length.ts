// corpus: accept/a369-grapheme-length
// purpose: graphemeLength counts extended grapheme clusters, and length counts UTF-8 bytes.
// exercises: text-module, grapheme, utf-8
// questions: compiler.md §193 rule 1, compiler.md §193 rule 2, Q5
// tsc: accepts; js-comparable: no C26: node cannot load the subscript:text module.
import { graphemeLength } from "subscript:text";

function show(label: string, s: string): void {
  print(`${label}: ${graphemeLength(s)} clusters, ${s.length} bytes`);
}

export function main(): void {
  show("empty", "");
  show("ascii", "Hello, world");
  show("japanese", "日本語のテキスト");
  show("supplementary kanji", "𠮷野家");
  // "か" + U+3099 COMBINING KATAKANA-HIRAGANA VOICED SOUND MARK.
  show("combining dakuten", "か\u3099き\u3099");
  show("precomposed dakuten", "がぎ");
  // Woman, woman, girl, boy, joined by U+200D ZERO WIDTH JOINER.
  show("zwj family", "\u{1F469}\u200D\u{1F469}\u200D\u{1F467}\u200D\u{1F466}");
  // Regional indicators J and P: one flag.
  show("regional pair", "\u{1F1EF}\u{1F1F5}");
  show("two flags", "\u{1F1EF}\u{1F1F5}\u{1F1FA}\u{1F1F8}");
  show("crlf", "a\r\nb");
}
