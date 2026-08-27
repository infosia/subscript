// corpus: accept/a150-receiver-address-invalidation
// purpose: Recomputes a CStruct array-element receiver address after an argument grows the same array.
// exercises: CStruct, dynamic-array, method-receiver, argument-evaluation, address-invalidation
// questions: §68

@CStruct
class V {
  a: i32;

  constructor(a: i32) {
    this.a = a;
  }

  bump(n: i32): i32 {
    return this.a + n;
  }
}

function growSync(xs: V[], n: i32): i32 {
  for (let i: i32 = 0; i < 64; i = i + 1) {
    xs.push(new V(100 + i));
  }
  return n;
}

export function main(): void {
  const zs: V[] = [new V(7)];
  print(`sync=${zs[0].bump(growSync(zs, 5))}`);
  const ws: V[] = [new V(7)];
  print(`ctl=${ws[0].bump(5)}`);
}
