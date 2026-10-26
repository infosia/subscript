// corpus: accept/a375-array-pop-shift-undefined
// purpose: pop and shift on a reference element give undefined on an empty array, as the lib types them.
// exercises: undefined, nullable, Array.pop, Array.shift, nullish, equality
// questions: compiler.md §196
// tsc: accepts; js-comparable: yes
class Job {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

function drain(jobs: Job[]): i32 {
  let sum: i32 = 0;
  let job = jobs.pop();
  while (job !== undefined) {
    sum += job.id;
    job = jobs.pop();
  }
  return sum;
}

export function main(): void {
  const jobs: Job[] = [new Job(1), new Job(2), new Job(3)];
  const first = jobs.shift();
  if (first !== undefined) {
    print(`first ${first.id}`);
  }
  const last = jobs.pop();
  print(`last ${last === undefined ? -1 : last.id}`);
  print(`sum ${drain(jobs)}`);
  print(`${jobs.pop() === undefined} ${jobs.shift() == null}`);
  const fallback = jobs.pop() ?? new Job(9);
  print(`fallback ${fallback.id}`);
  const counts: i32[] = [4, 5];
  print(`${counts.pop()} ${counts.shift()}`);
}
