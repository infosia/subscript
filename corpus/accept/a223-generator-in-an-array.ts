// corpus: accept/a223-generator-in-an-array
// purpose: Stores generators in an array and drives them from the array.
// observable: each element keeps its own suspended state across the reads.
// exercises: generator, escaping-value, array-element, generator-identity
// questions: Q30, compiler section 106
// tsc: accepts; js-comparable: yes
function* upTo(first: i32, last: i32): Generator<i32> {
  for (let value: i32 = first; value <= last; value += 1) {
    yield value;
  }
}

function step(source: Generator<i32>): i32 {
  const result = source.next();
  if (result.done) {
    return -1;
  }
  return result.value;
}

export function main(): void {
  const sources: Generator<i32>[] = [upTo(1, 3), upTo(10, 12)];
  print(`first ${step(sources[0])} second ${step(sources[1])}`);
  print(`first ${step(sources[0])} second ${step(sources[1])}`);

  let out: string = "";
  for (const value of sources[0]) {
    out += `${value},`;
  }
  print(`rest ${out}`);
}
