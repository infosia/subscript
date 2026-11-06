// corpus: reject/r490-and-left-operand-has-no-context
// purpose: The left operand of && takes no context, so a type parameter that only its result names has no candidate.
// exercises: generic-inference, contextual-type, logical-and
// questions: compiler.md §232.1 rule 4, §212.1 rule 5, §149
// tsc: accepts
// expected-error: S100 at line 13, cannot infer type parameter `T` of `pick`

function pick<T>(text: string): T {
  return JSON.parse<T>(text);
}

function both(c: boolean): boolean {
  if (pick("true") && c) {
    return true;
  }
  return false;
}

export function main(): void {
  print(`${both(true)}`);
}
