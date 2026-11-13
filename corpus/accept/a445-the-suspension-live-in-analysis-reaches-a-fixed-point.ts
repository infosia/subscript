// corpus: accept/a445-the-suspension-live-in-analysis-reaches-a-fixed-point
// purpose: Suspension live-in analysis ends for each loop form.
// observable: Each form prints end 2.
// exercises: async, generator, for-of, break, continue, while
// questions: compiler.md §273
// tsc: accepts; js-comparable: yes
function* gen(): Generator<i32> { yield 1; yield 2; }
export async function main(): Promise<void> {
  {
    let n: i32 = 0;
    for (let r: i32 = 0; r < 1; r++) {
      for (const j of [0, 1]) { if (j > 0) { break; } }
      for (const x of gen()) { await Context.suspend(); n++; }
    }
    print(`end ${n}`);
  }
  {
    let n: i32 = 0;
    for (let r: i32 = 0; r < 1; r++) {
      for (const j of [0, 1]) { if (j > 0) { continue; } }
      for (const x of gen()) { await Context.suspend(); n++; }
    }
    print(`end ${n}`);
  }
  {
    let n: i32 = 0;
    let r: i32 = 0;
    while (r < 1) {
      r++;
      for (const j of [0, 1]) { if (j > 0) { break; } }
      for (const x of gen()) { await Context.suspend(); n++; }
    }
    print(`end ${n}`);
  }
  {
    let n: i32 = 0;
    for (let r: i32 = 0; r < 1; r++) {
      for (const j of [0, 1]) { if (j > 0) {} }
      for (const x of gen()) { await Context.suspend(); n++; }
    }
    print(`end ${n}`);
  }
}
