// corpus: trap/t111-pad-end-cut-inside-sequence
// purpose: A padEnd cut inside a UTF-8 sequence of the pad traps before the result exists.
// exercises: padEnd, utf-8, string-slice
// questions: compiler.md §191 rule 1, Q5, Q21
// tsc: accepts
// js-comparable: no Q5 Q21: JavaScript counts the target in UTF-16 units and gives "Aあ".
// tier-policy: all three tiers trap
// expected-trap: string-slice at 13:23; message padEnd(2): the cut is at pad byte 1, inside a UTF-8 sequence
export function main(): void {
  const whole: string = "A".padEnd(4, "あ");
  print(`${whole} ${whole.length}`);
  print("before");
  const cut: string = "A".padEnd(2, "あ");
  print(`${cut.length}`);
}

// pin: 3112f7f7
// pin-dev-jit: no trap; stdout "Aあ 4", "before", "2"; exit 0.
// pin-c-aot: no trap; stdout "Aあ 4", "before", "2"; exit 0.
// pin-interpreter: no trap; stdout "Aあ 4", "before", "2".
// tsc-version: TypeScript 5.9.2; exit 0.
