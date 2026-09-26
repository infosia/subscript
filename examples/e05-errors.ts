// example: e05-errors
// teaches: Throw and catch the Error family, catch JSON.parse failures, return result-shaped values, and tell an exception from a trap.
// differs-from-typescript: C6 throws only Error, SyntaxError, and TypeError objects, rejects finally, and a trap is not catchable.
// see: corpus/accept/a18-error-handling.ts, corpus/accept/a250-throw-catch.ts, corpus/accept/a251-exception-propagation.ts, corpus/accept/a255-json-parse-direct.ts, corpus/trap/t02-statements-after-fault.ts, corpus/trap/t61-uncaught-exception.ts, corpus/reject/r11-throw.ts, corpus/reject/r233-finally.ts, corpus/reject/r236-caught-binding-use.ts, collisions.md C6, collisions.md Q28, compiler.md §115, compiler.md §19.3

// Failure has three forms here: an exception that a handler catches, a
// result value that the caller inspects, and a trap that the host observes.
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

// A result-shaped return: one function, two outcomes, one return type. The
// flag states which outcome the value carries. This stays one accepted
// style beside exceptions (C6).
function divide(numerator: f64, denominator: f64): DivisionResult {
  if (denominator === 0.0) {
    return new DivisionResult(false, 0.0);
  }
  return new DivisionResult(true, numerator / denominator);
}

// compiler.md §115.2: `throw` takes an Error, SyntaxError, or TypeError
// object. `throw "text"` is S010; corpus/reject/r11-throw.ts pins it.
function ratio(numerator: f64, denominator: f64): f64 {
  if (denominator === 0.0) {
    throw new Error("division by zero");
  }
  return numerator / denominator;
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // Section one: the caller reads both outcomes of a result value in order.
  const quotient: DivisionResult = divide(21.0, 3.0);
  const divisionFailure: DivisionResult = divide(1.0, 0.0);
  print(`division=${quotient.ok},${quotient.value}`);
  print(`division-error=${divisionFailure.ok}`);

  // Section two: an exception leaves `ratio` and every statement after the
  // raise site, up to the nearest handler (compiler.md §115.3 rule 6).
  // The catch binding has two uses: `instanceof` and `throw` (rule 4).
  // `finally` is S010; corpus/reject/r233-finally.ts pins it.
  try {
    print(`ratio=${ratio(21.0, 3.0)}`);
    print(`ratio=${ratio(1.0, 0.0)}`);
    print("not reached");
  } catch (e) {
    if (e instanceof Error) {
      print(`caught=${e.name}: ${e.message}`);
    }
  }

  // Section three: Q28 and compiler.md §115.7: JSON.parse returns T.
  // Malformed text raises SyntaxError with the byte offset, and a document
  // that does not match T raises TypeError. No partial value is visible.
  const parsed: i32 = JSON.parse("42");
  print(`json=${parsed}`);
  try {
    const malformed: i32 = JSON.parse<i32>("[");
    print(`json=${malformed}`);
  } catch (e) {
    if (e instanceof SyntaxError) {
      print(`json-syntax=${e.message}`);
    }
  }
  try {
    const mismatch: i32 = JSON.parse<i32>('"42"');
    print(`json=${mismatch}`);
  } catch (e) {
    if (e instanceof TypeError) {
      print(`json-type=${e.message}`);
    }
  }

  // Section four: C6 and compiler.md §19.3: a trap is not an exception. An
  // index out of bounds, a failed narrowing, or an allocation failure stops
  // the Context at the fault, and no `catch` runs. An exception that no
  // handler catches becomes the trap UncaughtException at the host entry
  // (compiler.md §115.4). compiler.md §18.2c makes the trap record
  // host-observable. corpus/trap/t02-statements-after-fault.ts and
  // corpus/trap/t61-uncaught-exception.ts pin both without trapping here.
}
