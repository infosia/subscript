// corpus: accept/a224-generator-as-a-map-value
// purpose: Stores generators as Map values and drives them from the map.
// observable: each map value keeps its own suspended state across the reads.
// exercises: generator, escaping-value, map-values, for-of-views
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
  const sources: Map<i32, Generator<i32>> = new Map<i32, Generator<i32>>();
  sources.set(1, upTo(1, 3));
  sources.set(2, upTo(10, 12));

  let round: string = "";
  for (const source of sources.values()) {
    round += `${step(source)},`;
  }
  print(`first ${round}`);

  round = "";
  for (const source of sources.values()) {
    round += `${step(source)},`;
  }
  print(`second ${round}`);
}
