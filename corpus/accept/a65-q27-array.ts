// corpus: accept/a65-q27-array
// purpose: Exercises the Q27 stage 3 Array additions: right-to-left
//          reduction, delete-only splice result and mutation, shift,
//          one-element unshift, and negative/clamped copyWithin.
// exercises: array-methods, runtime-to-script-calls, structural-mutation
// questions: Q27
// tsc: accepts
export function main(): void {
  const letters: string[] = ["a", "b", "c"];
  const right: string = letters.reduceRight(
    (acc: string, value: string): string => acc + value,
    "",
  );
  print(`reduceRight ${right}`);

  const middle: i32[] = [1, 2, 3, 4, 5];
  const removedMiddle: i32[] = middle.splice(1, 2);
  print(`splice ${removedMiddle.join(",")} | ${middle.join(",")}`);

  const negative: i32[] = [1, 2, 3, 4, 5];
  const removedNegative: i32[] = negative.splice(-2, 2);
  print(`spliceNeg ${removedNegative.join(",")} | ${negative.join(",")}`);

  const past: i32[] = [1, 2, 3];
  const removedPast: i32[] = past.splice(99, 5);
  print(`splicePast [${removedPast.join(",")}] | ${past.join(",")}`);

  const countPast: i32[] = [1, 2, 3];
  const removedCountPast: i32[] = countPast.splice(1, 99);
  print(`splicePastCount ${removedCountPast.join(",")} | ${countPast.join(",")}`);

  const shifted: i32[] = [1, 2, 3];
  print(`shift ${shifted.shift()} | ${shifted.join(",")}`);

  const prepended: i32[] = [1];
  print(`unshift ${prepended.unshift(0)} | ${prepended.join(",")}`);

  const copied: i32[] = [1, 2, 3, 4, 5];
  const copyResult: i32[] = copied.copyWithin(-2, -4, -3);
  print(`copyWithin ${copyResult.join(",")} | ${copied.join(",")}`);
  copyResult.fill(9, 0, 1);
  print(`copyAlias ${copyResult.join(",")} | ${copied.join(",")}`);
}
