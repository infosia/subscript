// corpus: reject/r114-switch-alias-duplicate-member
// purpose: Rejects a repeated member label in a Q32 switch.
// exercises: string-literal-union, switch-case, duplicate-member
// questions: Q32, R14
// tsc: accepts
// expected-error: S100 at the duplicate case label

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
    case "running":
      result = 21;
      break;
    case "done":
      result = 30;
      break;
  }
  return result;
}

export function main(): void {
  print(`${classify("queued")}`);
}
