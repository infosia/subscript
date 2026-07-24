// corpus: accept/a41-math-random
// purpose: Pins the Math.random contract sequence (stdlib.md §2):
//          xoshiro256++ seeded by splitmix64 expansion of the default
//          Context seed, top 53 bits mapped to [0, 1).
// exercises: math-random, context-prng
// questions: Q14, Q19

export function main(): void {
  for (let i: i32 = 0; i < 8; i++) {
    print(`${Math.random()}`);
  }
}
