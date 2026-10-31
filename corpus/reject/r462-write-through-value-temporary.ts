// corpus: reject/r462-write-through-value-temporary
// purpose: A write through a FixedArray or value class call result changes a copy that nothing reads, so it is rejected with a text that names the copy and states the fix.
// exercises: fixed-array, value-struct, assignment, compound-assignment, logical-assignment, update-expression
// questions: compiler.md §218, C2
// tsc: accepts
// expected-error: S100, this write changes a copy of the `FixedArray<i32, 2>` result, and nothing reads that copy; bind the result to a variable, or assign to the source
@ValueType
class V {
  x: i32 = 1;
  a: FixedArray<i32, 2> = [1, 2];
}

class B {
  values: FixedArray<i32, 2> = [1, 2];
  held: V = new V();
  gt(): FixedArray<i32, 2> {
    return this.values;
  }
  vv(): V {
    return this.held;
  }
}

function cur(): FixedArray<i32, 2> {
  return [1, 2];
}

function cv(): V {
  return new V();
}

function pair(): FixedArray<string, 2> {
  return ["a", "b"];
}

function flags(): FixedArray<boolean, 2> {
  return [false, true];
}

function names(): FixedArray<string | null, 2> {
  return [null, "b"];
}

export function main(): void {
  const b = new B();
  cur()[0] = 9;
  cv().x = 3;
  cv().a[1] = 7;
  b.gt()[1] = 5;
  b.vv().x = 5;
  b.vv().a[0] = 5;
  cur()[0]++;
  --cur()[1];
  cur()[0] += 2;
  cv().a[0] *= 2;
  flags()[0] ||= true;
  names()[0] ??= "zz";
  pair()[0] = "zz";
}
