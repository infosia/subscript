// corpus: accept/a61-same-value-zero
// purpose: Pins the Q22/Q24 SameValueZero revision for float array
//          includes and associative keys, including NaN payload
//          unification, literal NaN keys, and stored -0 normalization.
// exercises: includes-same-value-zero, map-set-nan-keys, zero-key-normalization
// questions: Q22, Q24
// tsc: accepts; js-comparable: no Q24: Map.getOr has no JavaScript shim.
let zeroKeys: string = "";

export function main(): void {
  const zero: f64 = 0.0;
  const dividedNaN: f64 = zero / zero;
  const parsedNaN: f64 = parseFloat("not-a-number");

  const f64NaNs: f64[] = [dividedNaN];
  print(`f64 array ${f64NaNs.includes(NaN)} ${f64NaNs.indexOf(NaN)} ${f64NaNs.lastIndexOf(NaN)}`);

  const f32NaN: f32 = dividedNaN as f32;
  const f32Needle: f32 = NaN as f32;
  const f32NaNs: f32[] = [f32NaN];
  print(`f32 array ${f32NaNs.includes(f32Needle)} ${f32NaNs.indexOf(f32Needle)} ${f32NaNs.lastIndexOf(f32Needle)}`);

  const map: Map<f64, i32> = new Map<f64, i32>();
  map.set(NaN, 11);
  map.set(dividedNaN, 22);
  map.set(parsedNaN, 33);
  print(`map ${map.getOr(NaN, -1)} ${map.has(parsedNaN)} ${map.size}`);

  const set: Set<f64> = new Set<f64>();
  set.add(NaN);
  set.add(dividedNaN);
  set.add(parsedNaN);
  print(`set ${set.has(NaN)} ${set.has(dividedNaN)} ${set.size}`);

  const zeros: Map<f64, i32> = new Map<f64, i32>();
  zeros.set(-0.0, 1);
  zeros.set(0.0, 2);
  zeros.forEach((value: i32, key: f64): void => {
    zeroKeys += `${key}:${value}`;
  });
  print(`zero ${zeroKeys} ${zeros.size}`);
}
