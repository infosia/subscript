// corpus: accept/a115-switch-literal-union
// purpose: Exercises integer-dispatched switch statements over a closed Q32 alias.
// exercises: string-literal-union, exhaustive-switch, default-subset, integer-dispatch
// questions: Q32, R14

type Phase = "queued" | "running" | "done";

function exhaustive(phase: Phase): i32 {
  let result: i32 = 0;
  switch (phase) {
    case "queued":
      result = 10;
      break;
    case "running":
      result = 20;
      break;
    case "done":
      result = 30;
      break;
  }
  return result;
}

function subset(phase: Phase): i32 {
  let result: i32 = 0;
  switch (phase) {
    case "running":
      result = 200;
      break;
    default:
      result = -1;
      break;
  }
  return result;
}

export function main(): void {
  const queued: Phase = "queued";
  const running: Phase = "running";
  const done: Phase = "done";

  print(`queued=${exhaustive(queued)}/${subset(queued)}`);
  print(`running=${exhaustive(running)}/${subset(running)}`);
  print(`done=${exhaustive(done)}/${subset(done)}`);
}
