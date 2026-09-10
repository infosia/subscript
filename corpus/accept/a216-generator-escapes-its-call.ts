// corpus: accept/a216-generator-escapes-its-call
// purpose: Returns a Generator from a call, passes it on, and drives it two ways.
// exercises: generator, escaping-value, for-of
// questions: Q30, compiler section 103
// tsc: accepts; js-comparable: yes
function* upTo(first: i32, last: i32): Generator<i32> {
  for (let value: i32 = first; value <= last; value += 1) {
    yield value;
  }
}

function make(first: i32, last: i32): Generator<i32> {
  return upTo(first, last);
}

function drain(source: Generator<i32>): string {
  let out: string = "";
  for (const value of source) {
    out += `${value},`;
  }
  return out;
}

function step(source: Generator<i32>): i32 {
  const result = source.next();
  if (result.done) {
    return -1;
  }
  return result.value;
}

export function main(): void {
  print(`returned ${drain(make(1, 3))}`);

  const passed: Generator<i32> = upTo(4, 6);
  print(`passed ${drain(passed)}`);

  const stepped: Generator<i32> = upTo(7, 8);
  print(`stepped ${step(stepped)} ${step(stepped)} ${step(stepped)}`);
}
