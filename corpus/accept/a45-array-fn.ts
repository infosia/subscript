// corpus: accept/a45-array-fn
// purpose: Exercises the closure-taking Array method subset of
//          stdlib.md §9 (Q22) — the runtime→script call machinery:
//          map with an element-type change (f64 -> string), a named
//          function reference as a callback (C5), filter, reduce with
//          the required init (numeric sum and a string fold whose acc
//          type differs from T), some/every with the short-circuit
//          order pinned by a side-effect counter, findIndex hit and
//          miss, forEach accumulation, and sort with the required
//          comparator — numeric ascending plus stability (equal keys
//          keep input order, pinned by the seq digits).
// exercises: array-methods, closures, runtime-to-script-calls
// questions: Q22, C5
// tsc: accepts; js-comparable: no Q14: Negative-zero formatting produces different output.
let probes: i32 = 0;
let sum: f64 = 0;

function isEven(v: i32): boolean {
  return v % 2 === 0;
}

class Pair {
  key: i32;
  seq: i32;
  constructor(key: i32, seq: i32) {
    this.key = key;
    this.seq = seq;
  }
}

export function main(): void {
  // map: f64 -> string — the element type changes; Q14 formatting
  // (shortest round-trip, -0) inside the closure body.
  const fs: f64[] = [0.5, 2, -0];
  const mapped: string[] = fs.map((v: f64): string => `<${v}>`);
  print(`map ${mapped.join(" ")}`);
  // map: i32 -> i32 with an inferred-return expression body.
  const ns: i32[] = [1, 2, 3, 4, 5];
  print(`map2 ${ns.map((v: i32) => v * 2).join(",")}`);
  // filter: a named function reference is a callback too (C5), and so
  // is an inline predicate.
  print(`filter ${ns.filter(isEven).join(",")}`);
  print(`filter2 ${ns.filter((v: i32): boolean => v > 3).join(",")}`);
  // reduce: init is required (Q22); the acc type may differ from T.
  print(`reduce ${ns.reduce((acc: i32, v: i32): i32 => acc + v, 100)}`);
  const folded: string = ns.reduce((acc: string, v: i32): string => acc + `${v}`, "#");
  print(`fold ${folded}`);
  // some/every short-circuit: the probe counter pins how many elements
  // the predicate observed before stopping.
  probes = 0;
  const anyBig: boolean = ns.some((v: i32): boolean => {
    probes += 1;
    return v >= 3;
  });
  print(`some ${anyBig} probes ${probes}`);
  probes = 0;
  const allSmall: boolean = ns.every((v: i32): boolean => {
    probes += 1;
    return v < 2;
  });
  print(`every ${allSmall} probes ${probes}`);
  // findIndex: hit and miss.
  print(`findIndex ${ns.findIndex((v: i32): boolean => v > 3)} ${ns.findIndex((v: i32): boolean => v > 99)}`);
  // forEach: accumulate into a module global.
  sum = 0;
  fs.forEach((v: f64): void => {
    sum += v;
  });
  print(`forEach ${sum}`);
  // sort: comparator required (Q22); ascending numbers; in place and
  // the receiver is the expression's value.
  const us: i32[] = [5, 1, 4, 2, 3];
  const us2: i32[] = us.sort((a: i32, b: i32): i32 => a - b);
  print(`sort ${us.join(",")} ${us2.join(",")}`);
  // sort stability: pairs sorted by key keep the input order of their
  // seq values within each equal key (stable merge sort, §9).
  const ps: Pair[] = [
    new Pair(2, 0),
    new Pair(1, 1),
    new Pair(2, 2),
    new Pair(1, 3),
    new Pair(1, 4),
  ];
  ps.sort((a: Pair, b: Pair): i32 => a.key - b.key);
  print(`stable ${ps.reduce((acc: string, p: Pair): string => acc + `${p.seq}`, "")}`);
}
