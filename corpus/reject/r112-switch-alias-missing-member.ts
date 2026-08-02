// corpus: reject/r112-switch-alias-missing-member
// purpose: Rejects a default-free Q32 switch that omits one alias member.
// exercises: string-literal-union, switch-exhaustiveness, missing-member
// questions: Q32, R14
// tsc-clean-standalone: verified with node_modules/.bin/tsc --noEmit --strict --target es2022 --lib es2022 against prelude/lang.d.ts; stock TypeScript accepts non-exhaustive alias switches.
// expected-error: S100 at the non-exhaustive switch

type Phase = "queued" | "running" | "done";

function classify(phase: Phase): i32 {
  let result: i32 = 0;
  switch (phase) {
    case "queued":
      result = 10;
      break;
    case "running":
      result = 20;
      break;
  }
  return result;
}

export function main(): void {
  print(`${classify("queued")}`);
}
