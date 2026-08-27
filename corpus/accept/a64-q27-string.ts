// corpus: accept/a64-q27-string
// purpose: Exercises every Q27 Stage 2 String addition and closes the
//          former replacement-pattern divergence.
// exercises: substring, substr, char-at, code-point-at, string-concat,
//            positioned-prefix-suffix, replacement-substitutions
// questions: Q5, Q21, Q27
// tsc: accepts
export function main(): void {
  const word: string = "hello";
  // Side-by-side comparisons are the proof that substring is not a
  // duplicate of slice: reversed arguments swap, and negatives clamp.
  print(`reversed substring [${word.substring(4, 1)}] slice [${word.slice(4, 1)}]`);
  print(`negative substring [${word.substring(-2, 3)}] slice [${word.slice(-2, 3)}]`);

  print(`substr negative [${word.substr(-2)}]`);
  print(`substr nonpositive [${word.substr(2, 0)}] [${word.substr(2, -1)}]`);

  print(`charAt [${"ABC".charAt(1)}] [${"ABC".charAt(9)}] [${"aéx".charAt(1)}]`);
  print(`codePointAt ${"ABC".codePointAt(0)} ${"aéx".codePointAt(1)}`);
  print(`concat ${"hello".concat("!")}`);
  print(`position ${word.startsWith("ll", 2)} ${word.endsWith("hel", 3)}`);

  print(`replace ${"a-b".replace("-", "[$$][$&][$`][$'][$1]")}`);
  print(`replaceAll ${"a-b-c".replaceAll("-", "<$$|$&|$`|$'|$1>")}`);
}
