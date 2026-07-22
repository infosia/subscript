// corpus: accept/a09-enums
// purpose: Declares and uses a numeric enum with stable values.
// exercises: numeric-enum, enum-comparison, c-enum-lowering
// questions: Q1, Q12

enum Status {
  Ready = 1,
  Running = 2,
  Complete = 3,
}

function statusCode(status: Status): i32 {
  return status as i32;
}

export function main(): void {
  const status: Status = Status.Complete;
  print(`${statusCode(status)}`);
}
