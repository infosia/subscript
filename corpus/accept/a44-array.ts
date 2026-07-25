// corpus: accept/a44-array
// purpose: Exercises the no-closure Array method subset of stdlib.md §9
//          (Q22): indexOf/lastIndexOf per-kind `===` equality (i32 by
//          value, f64 IEEE, string by content, Date by millis, reference
//          classes by identity) contrasted with includes, which uses
//          SameValueZero and therefore finds NaN; join
//          with Q14 formatting and the "," default separator, slice
//          with the JS negative/clamp rules, fill/reverse in place
//          returning the receiver, and concat of exactly one array.
// exercises: array-methods, q14-formatting
// questions: Q14, Q22

class Obj {
  tag: i32;
  constructor(tag: i32) {
    this.tag = tag;
  }
}

export function main(): void {
  // i32[]: equality by value; hit, duplicate, miss.
  const xs: i32[] = [4, 7, 4, 9];
  print(`iof ${xs.indexOf(4)} ${xs.indexOf(9)} ${xs.indexOf(5)}`);
  print(`liof ${xs.lastIndexOf(4)} ${xs.lastIndexOf(5)}`);
  const has7: boolean = xs.includes(7);
  print(`inc ${has7} ${xs.includes(5)}`);
  // f64[]: -0 equals 0 for every search; `indexOf` uses `===` and so
  // never finds NaN, while `includes` uses SameValueZero and does.
  const fs: f64[] = [0, 1.5, Math.sqrt(-1)];
  print(`fiof ${fs.indexOf(-0)} ${fs.indexOf(1.5)} ${fs.indexOf(Math.sqrt(-1))}`);
  // SameValueZero: `includes` finds NaN where `indexOf` above does not
  // (Q22, 2026-07-25). a61 isolates the rule; this line keeps the two
  // searches side by side, which is where the divergence is visible.
  print(`finc ${fs.includes(Math.sqrt(-1))} ${fs.includes(2.5)}`);
  // string[]: equality by content — split-produced strings match fresh
  // literals and concatenations (never pointer identity).
  const parts: string[] = "alpha,beta,alpha".split(",");
  print(`siof ${parts.indexOf("beta")} ${parts.indexOf("al" + "pha")}`);
  print(`sliof ${parts.lastIndexOf("alpha")} ${parts.indexOf("gamma")}`);
  // Date[]: equality by millis (distinct objects, same instant).
  const ds: Date[] = [new Date(1000), new Date(2000)];
  print(`diof ${ds.indexOf(new Date(2000))} ${ds.indexOf(new Date(3000))}`);
  // Reference classes: identity — a content-equal fresh instance is a
  // different reference.
  const a: Obj = new Obj(1);
  const b: Obj = new Obj(1);
  const os: Obj[] = [a, b];
  print(`oiof ${os.indexOf(b)} ${os.indexOf(new Obj(1))}`);
  // join: default "," separator, a custom separator, Q14 float
  // formatting (shortest round-trip, -0, NaN), booleans, strings.
  print(`join ${xs.join()}`);
  print(`joinsep ${xs.join(" | ")}`);
  print(`joinf ${fs.join(",")}`);
  const bs: boolean[] = [true, false, true];
  print(`joinb ${bs.join(",")}`);
  print(`joins ${parts.join("+")}`);
  // slice: JS negatives and clamps; a fresh array, receiver untouched.
  const sl: i32[] = [10, 20, 30, 40, 50];
  print(`slice ${sl.slice(1, 3).join(",")}`);
  print(`sliceneg ${sl.slice(-2).join(",")}`);
  print(`slicenegend ${sl.slice(0, -1).join(",")}`);
  print(`sliceall ${sl.slice().join(",")}`);
  print(`sliceclamp ${sl.slice(3, 99).join(",")} [${sl.slice(4, 2).join(",")}]`);
  print(`slicekeep ${sl.join(",")}`);
  // fill: in place, returns the receiver (the same array observed
  // through the returned handle), JS range clamps.
  const fl: i32[] = [1, 2, 3, 4, 5];
  const fl2: i32[] = fl.fill(0, 1, 3);
  print(`fill ${fl.join(",")} ${fl2.join(",")}`);
  fl.fill(7, -2);
  print(`fillneg ${fl.join(",")}`);
  fl.fill(9);
  print(`fillall ${fl.join(",")}`);
  // reverse: in place, returns the receiver.
  const rv: i32[] = [1, 2, 3, 4];
  const rv2: i32[] = rv.reverse();
  print(`rev ${rv.join(",")} ${rv2.join(",")}`);
  // concat: exactly one array argument; fresh array, operands kept.
  const c1: i32[] = [1, 2];
  const c2: i32[] = [3];
  const cc: i32[] = c1.concat(c2);
  print(`concat ${cc.join(",")} ${c1.join(",")} ${c2.join(",")}`);
}
