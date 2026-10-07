// corpus: accept/a344-zero-length-store
// purpose: A zero length store clears arrays in place and permits reuse.
// exercises: array-clear, counted-array, reference-array
// questions: compiler.md §174, §171
// tsc: accepts
// js-comparable: yes

class Item { value: i32 = 7; }
async function work(value: i32): Promise<i32> { return value; }
function clear(xs: i32[]): void { xs.length = 0; }
export async function main(): Promise<void> {
  const numbers: i32[] = [1, 2];
  numbers.length = 0;
  numbers.push(3);
  print(`number ${numbers[0]} ${numbers.length}`);
  const strings: string[] = ["old"];
  strings.length = 0;
  strings.push("new");
  print(`string ${strings[0]} ${strings.length}`);
  const refs: Item[] = [new Item()];
  refs.length = 0;
  refs.push(new Item());
  print(`reference ${refs[0].value} ${refs.length}`);
  const jobs: Promise<i32>[] = [work(4), work(5)];
  await jobs[0]; await jobs[1];
  jobs.length = 0;
  jobs.push(work(6));
  print(`handle ${await jobs[0]} ${jobs.length}`);
  clear(numbers);
  print(`parameter ${numbers.length}`);
}

// pin: 69212b29
// pin-dev-jit: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-c-aot: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-interpreter: Checker rejects with S100 before execution.
