// corpus: reject/r112-switch-alias-missing-member
// purpose: Rejects a default-free Q32 switch that omits one alias member.
// exercises: string-literal-union, switch-exhaustiveness, missing-member
// questions: Q32, R14
// tsc: accepts
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
