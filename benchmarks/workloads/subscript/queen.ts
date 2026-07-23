// benchmark: queen
// Count solutions to the 13-queens problem by bitmask backtracking.
// Checksum: the solution count, i32 = 73712.

const N: i32 = 13;

function solve(cols: i32, ld: i32, rd: i32, all: i32): i32 {
  if (cols === all) {
    return 1;
  }
  let count: i32 = 0;
  let poss: i32 = ~(cols | ld | rd) & all;
  while (poss !== 0) {
    const p: i32 = poss & (0 - poss);
    poss = poss - p;
    count += solve(cols | p, (ld | p) << 1, (rd | p) >> 1, all);
  }
  return count;
}

export function main(): void {
  // The board width is read through the runtime array (opaque to the
  // optimizer) so the backtracking is evaluated, not folded to 73712.
  const seed: i32[] = [N];
  const bits: i32 = seed[0];
  const all: i32 = (1 << bits) - 1;
  const checksum: i32 = solve(0, 0, 0, all);
  print(`${checksum}`);
}
