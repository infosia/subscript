// corpus: reject/r480-element-nested-receiver
// purpose: A value field of an element and an element of its FixedArray field are unstable receivers.
// exercises: value-struct, dynamic-array, method-receiver, fixed-array, nested-field
// questions: compiler.md §221.1 rule 1
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
const all: V[] = [];

@ValueType
class Inner {
  n: i32 = 1;

  grow(): void {
    all.push(new V());
  }
}

@ValueType
class V {
  inner: Inner = new Inner();
  a: FixedArray<Inner, 2> = [new Inner(), new Inner()];
}

export function main(): void {
  all.push(new V());
  all[0].a[1].grow();
  all[0].inner.grow();
}
