// example: e02-value-and-reference
// teaches: Place a C-layout value class beside a reference class and observe copies on assignment and calls.
// differs-from-typescript: C2 makes @CStruct classes values; C1 makes every class declaration nominal.
// see: corpus/accept/a04-value-struct.ts, corpus/accept/a05-nominal-identity.ts, corpus/accept/a21-methods.ts, corpus/accept/a56-map-aggregate-foreach.ts, corpus/reject/r06-structural-substitution.ts, corpus/reject/r07-value-class-extends.ts, collisions.md C1-C2

// Two classes follow with the same single field. The decorator is the only
// difference between them, and it decides what an assignment does.
// C2: @CStruct makes this a C-layout value copied on assignment and pass.
// Rejected alternative: adding extends is S006, "value classes do not
// inherit"; corpus/reject/r07-value-class-extends.ts pins it.
@CStruct
class ValueSwitch {
  enabled: boolean;

  constructor(enabled: boolean) {
    this.enabled = enabled;
  }
}

// C2: without @CStruct this is a Context-allocated reference class.
class ReferenceSwitch {
  enabled: boolean;

  constructor(enabled: boolean) {
    this.enabled = enabled;
  }
}

// A call boundary. The program needs one, because a pass copies a value class
// exactly as an assignment copies it.
// C1: the identical field shape does not make ReferenceSwitch substitutable
// for ValueSwitch. Rejected alternative: passing one here is S005;
// diagnostic excerpt: "nominal types are not interchangeable";
// corpus/reject/r06-structural-substitution.ts pins it.
function flipPassed(value: ValueSwitch): boolean {
  value.enabled = !value.enabled;
  return value.enabled;
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // Case one: assignment. The copy and the original are separate storage, and
  // assigned=true,false shows that the write reached the copy only.
  const original: ValueSwitch = new ValueSwitch(true);
  // C2: assignment copies the value, so this mutation does not reach original.
  const assigned: ValueSwitch = original;
  assigned.enabled = false;
  print(`assigned=${original.enabled},${assigned.enabled}`);

  // Case two: a call. passed=false,true reports the callee's own copy beside
  // the unchanged original.
  // C2: the argument is another copy, so mutation inside flipPassed does not
  // reach original.
  const passed: boolean = flipPassed(original);
  print(`passed=${passed},${original.enabled}`);

  // Case three: a reference class. reference=false shows one instance behind
  // the name, so the write is visible through it.
  const reference: ReferenceSwitch = new ReferenceSwitch(true);
  reference.enabled = false;
  print(`reference=${reference.enabled}`);
  // Q6: reference-class lifetime ends explicitly at Context.free.
  Context.free(reference);
}
