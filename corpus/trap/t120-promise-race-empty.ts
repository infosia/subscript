// corpus: trap/t120-promise-race-empty
// purpose: A race over an empty Promise<i32>[] value traps at the call, after the output before it.
// exercises: promise-race, array, trap-position
// questions: compiler.md §350 rule 7, §166
// tsc: accepts
// js-comparable: no C8: node does not settle a race over an empty array, so the program ends with no output after the race.
// tier-policy: all three tiers trap
// expected-trap: promise-race-empty at the Promise.race call (13:24); message Promise.race of an empty array

export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [];
  print("before");
  const r: i32 = await Promise.race(jobs);
  print(`${r}`);
}

// pin: 80b3f15f
// pin-dev-jit: Rejected at the check with S016 "unknown name `Promise`" at the race (13:24); no output. Measured at 80b3f15f.
// pin-c-aot: Rejected at the check with S016 "unknown name `Promise`" at the race (13:24); no build. Measured at 80b3f15f.
// pin-interpreter: Rejected at the check with S016 "unknown name `Promise`" at the race (13:24); no output. Measured at 80b3f15f.
// tsc-version: TypeScript 5.9.2; exit 0.
