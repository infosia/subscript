// corpus: accept/a18-error-handling
// purpose: Represents and checks a fallible operation with a result value.
// exercises: result-value, checked-error, no-throw
// questions: Q1, Q2, Q9, Q12
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct
class DivisionResult {
  ok: boolean;
  value: f64;

  constructor(ok: boolean, value: f64) {
    this.ok = ok;
    this.value = value;
  }
}

function divide(numerator: f64, denominator: f64): DivisionResult {
  if (denominator === 0.0) {
    return new DivisionResult(false, 0.0);
  }
  return new DivisionResult(true, numerator / denominator);
}

export function main(): void {
  const success: DivisionResult = divide(21.0, 3.0);
  const failure: DivisionResult = divide(1.0, 0.0);
  if (success.ok && !failure.ok) {
    print(`${success.value}`);
    return;
  }
  print("unexpected");
}
