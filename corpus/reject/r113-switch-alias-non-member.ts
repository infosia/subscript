// corpus: reject/r113-switch-alias-non-member
// purpose: Rejects a Q32 switch label outside the alias member set.
// exercises: string-literal-union, switch-case, closed-member-set
// questions: Q32, R14
// tsc-status: stock TypeScript also rejects the non-member label with TS2678; this is not a tsc-clean pin.
// expected-error: S100 at the non-member case label

type Phase = "queued" | "running" | "done";

function classify(phase: Phase): i32 {
  let result: i32 = 0;
  switch (phase) {
    case "queued":
      result = 10;
      break;
    case "paused":
      result = 20;
      break;
    default:
      result = -1;
      break;
  }
  return result;
}

export function main(): void {
  print(`${classify("queued")}`);
}
