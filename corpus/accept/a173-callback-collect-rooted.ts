// corpus: accept/a173-callback-collect-rooted
// purpose: A result array under construction survives Context.collect called from the callback.
// observable: map and filter through a function value print complete results after per-element collection.
// exercises: function-value-intrinsic, explicit-collection, runtime-result-rooting
// questions: Q7, Q22, Q27
// tsc: accepts; js-comparable: no Q7: Context.collect has no JavaScript equivalent.
export function main(): void {
  const xs: i32[] = [1, 2, 3, 4, 5, 6, 7, 8];
  const k: i32 = 10;
  const toText = (x: i32): string => {
    const s: string = `v${x * k}`;
    Context.collect();
    return s;
  };
  const ys: string[] = xs.map(toText);
  print(`map ${ys.length} ${ys[0]} ${ys[7]}`);
  const even = (x: i32): boolean => {
    Context.collect();
    return x % 2 === 0 && x > k - 10;
  };
  const zs: i32[] = xs.filter(even);
  print(`filter ${zs.length} ${zs[0]} ${zs[3]}`);
  const fixed: FixedArray<i32, 4> = [1, 2, 3, 4];
  const ws: i32[] = fixed.filter(even);
  print(`fixed ${ws.length} ${ws[1]}`);
}
