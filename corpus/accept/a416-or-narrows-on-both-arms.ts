// corpus: accept/a416-or-narrows-on-both-arms
// purpose: A fact that both arms of || give, or both arms of && give when false, narrows the path.
// observable: each form reads the narrowed path for the left arm and the right arm, and takes the other branch for neither arm.
// exercises: logical-or-narrowing, logical-and-narrowing, not-narrowing, conditional-expression, nullable-reference, shared-field-path, element-check
// questions: compiler.md §227, §219.1 rule 4, §124, C7
// tsc: accepts; js-comparable: yes
class C {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Holder {
  f: string | null;
  constructor(f: string | null) {
    this.f = f;
  }

  fieldOr(): i32 {
    if (this.f === "x" || this.f === "y") {
      return this.f.length;
    }
    return -1;
  }
}

function orIf(a: string | null): i32 {
  if (a === "x" || a === "y") {
    return a.length;
  }
  return -1;
}

function andFalse(a: string | null): i32 {
  if (a !== "xx" && a !== "y") {
    return -1;
  }
  return a.length;
}

function notOr(a: string | null): i32 {
  if (!(a === "xx" || a === "y")) {
    return -1;
  }
  return a.length;
}

function threeArms(a: string | null): i32 {
  if (a === "x" || a === "yy" || a === "zzz") {
    return a.length;
  }
  return -1;
}

function conditional(a: string | null): i32 {
  return a === "x" || a === "yy" ? a.length : 0;
}

function presentOrEqual(a: string | null): i32 {
  if (a !== null || a === "y") {
    return a.length;
  }
  return -1;
}

function classOr(a: C | null, b: C): i32 {
  if (a === b || a !== null) {
    return a.v;
  }
  return -1;
}

function elementOr(xs: (string | null)[]): i32 {
  const i: i32 = 0;
  if (xs[i] === "x" || xs[i] === "y") {
    return 1;
  }
  return -1;
}

export function main(): void {
  print(`orIf:${orIf("x")}:${orIf("y")}:${orIf("z")}:${orIf(null)}`);
  print(`andFalse:${andFalse("xx")}:${andFalse("y")}:${andFalse("z")}:${andFalse(null)}`);
  print(`notOr:${notOr("xx")}:${notOr("y")}:${notOr("z")}:${notOr(null)}`);
  print(`threeArms:${threeArms("x")}:${threeArms("yy")}:${threeArms("zzz")}:${threeArms("w")}:${threeArms(null)}`);
  print(`conditional:${conditional("x")}:${conditional("yy")}:${conditional("z")}:${conditional(null)}`);
  print(`presentOrEqual:${presentOrEqual("abcd")}:${presentOrEqual(null)}`);
  const b: C = new C(5);
  print(`classOr:${classOr(b, b)}:${classOr(new C(6), b)}:${classOr(null, b)}`);
  print(`fieldOr:${new Holder("x").fieldOr()}:${new Holder("y").fieldOr()}:${new Holder("z").fieldOr()}:${new Holder(null).fieldOr()}`);
  print(`elementOr:${elementOr(["x"])}:${elementOr(["y"])}:${elementOr(["z"])}:${elementOr([null])}`);
}
