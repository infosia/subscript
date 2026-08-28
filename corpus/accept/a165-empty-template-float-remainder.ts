// corpus: accept/a165-empty-template-float-remainder
// purpose: Emits an empty template and applies float remainder with the C fmod rules.
// exercises: empty-template, float-remainder, nan, infinity, signed-remainder
// questions: §68
// tsc: accepts; js-comparable: yes

export function main(): void {
  const empty: string = ``;
  const zero: f64 = 0.0;
  const infinity: f64 = 1.0 / zero;
  const nan: f64 = zero / zero;
  print(`empty=[${empty}] len=${empty.length}`);
  print(`positive=${5.5 % 2.0}`);
  print(`negative-left=${-5.5 % 2.0}`);
  print(`negative-right=${5.5 % -2.0}`);
  print(`zero=${5.5 % zero}`);
  print(`infinity-left=${infinity % 2.0}`);
  print(`infinity-right=${2.0 % infinity}`);
  print(`nan=${nan % 2.0}`);
}
