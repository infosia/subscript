// corpus: trap/t57-sandbox-alloc-quota
// profile: sandbox
// purpose: Passes the 64 MiB default allocation quota of the sandbox profile.
// exercises: sandbox-profile, allocation-quota, trap-stop
// questions: Q7, compiler section 109
// tier-policy: every tier traps with kind 26 at the slice allocation; the iteration that traps is tier-specific, so the entry prints nothing that depends on it
// expected-trap: allocation-quota at the slice allocation
export function main(): void {
  print("start");
  let seed: u8[] = [1];
  for (let step: i32 = 0; step < 18; step = step + 1) {
    seed = seed.concat(seed);
  }
  const blocks: u8[][] = [];
  for (let block: i32 = 0; block < 300; block = block + 1) {
    blocks.push(seed.slice(0, seed.length));
  }
  print(`${blocks.length}`);
}
