// example: e05-no-exceptions
// teaches: Return result-shaped values, inspect JsonResult before reading it, and distinguish data failure from a trap.
// differs-from-typescript: C6 rejects throw and try; fallible operations return data, while a trap stops the Context for the host.
// see: corpus/accept/a18-error-handling.ts, corpus/accept/a70-json-roundtrip.ts, corpus/accept/a71-json-parse.ts, corpus/accept/a72-json-parse-limits.ts, corpus/trap/t01-json-result-value.ts, corpus/trap/t02-statements-after-fault.ts, corpus/trap/t03-loop-stops-at-fault.ts, corpus/trap/t04-call-after-fault.ts, corpus/trap/t05-foreach-callback-fault.ts, corpus/reject/r11-throw.ts, collisions.md C6, collisions.md Q28, compiler.md §18.2c, compiler.md §19.3

// C2: @CStruct makes this a C-layout value rather than a Context-allocated
// reference class.
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
  // C6: an avoidable failure is ordinary data that the caller can inspect.
  // Rejected alternative: throw is S010, "exceptions are not in the
  // language; return a result value"; corpus/reject/r11-throw.ts pins it.
  if (denominator === 0.0) {
    return new DivisionResult(false, 0.0);
  }
  return new DivisionResult(true, numerator / denominator);
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  const quotient: DivisionResult = divide(21.0, 3.0);
  const divisionFailure: DivisionResult = divide(1.0, 0.0);
  print(`division=${quotient.ok},${quotient.value}`);
  print(`division-error=${divisionFailure.ok}`);

  // Q28: JSON syntax and type failures are JsonResult data, and value is
  // readable only after ok proves that parsing succeeded.
  const parsed: JsonResult<i32> = JSON.parse<i32>("42");
  if (parsed.ok) {
    print(`json=${parsed.value}`);
  }
  const malformed: JsonResult<i32> = JSON.parse<i32>("[");
  print(`json-error=${malformed.ok}`);

  // Q6/Q28: each JsonResult is a Context allocation released by its caller.
  unsafeDelete(parsed);
  unsafeDelete(malformed);

  // C6 and compiler.md §19.3: a trap is not a result value; it stops the
  // Context at the fault. compiler.md §18.2c makes the trap record
  // host-observable, and the examples.md §5 capstone demonstrates that
  // host-side check. corpus/trap/t01-t05 pin this without trapping here.
}
