// corpus: accept/a53-set
// purpose: Exercises the accepted Set battery plus Q24 float zero,
//          NaN insertion, and an ordinary float miss.
// exercises: set-methods, float-keys, deterministic-hashing
// questions: Q24, Q22

export function main(): void {
  const words: Set<string> = new Set<string>();
  print(`add receiver ${words.add("red") === words}`);
  words.add("blue");
  words.add("red");
  print(`words ${words.size} ${words.has("red")} ${words.has("green")}`);
  print(`delete ${words.delete("red")} ${words.delete("red")} ${words.size}`);
  words.clear();
  print(`clear ${words.size}`);

  const floats: Set<f64> = new Set<f64>();
  const minusZero: f64 = -0.0;
  const plusZero: f64 = 0.0;
  floats.add(minusZero);
  const zero: f64 = 0.0;
  const computedNaN: f64 = zero / zero;
  floats.add(computedNaN);
  print(`float ${floats.has(plusZero)} ${floats.has(1.0)} ${floats.size}`);
}
