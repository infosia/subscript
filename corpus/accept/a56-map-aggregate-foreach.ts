// corpus: accept/a56-map-aggregate-foreach
// purpose: Pins C2 copy-on-pass for aggregate Map.forEach values when
//          callbacks mutate their parameter or overwrite the visited entry.
// exercises: map-foreach, value-class-copy, fixed-array-copy, callback-abi
// questions: Q24, C2, C5
// tsc: accepts
@CStruct
class V3 {
  x: i32;
  y: i32;
  z: i32;
  constructor(x: i32, y: i32, z: i32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}

let structVisit: string = "";
let arrayVisit: string = "";

function mutateStruct(value: V3, _key: i32): void {
  value.x = 777;
}

function mutateArray(value: FixedArray<i32, 3>, _key: i32): void {
  value[0] = 777;
}

export function main(): void {
  const structMutate: Map<i32, V3> = new Map<i32, V3>();
  structMutate.set(1, new V3(1, 2, 3));
  structMutate.set(2, new V3(4, 5, 6));
  structMutate.forEach(mutateStruct);
  const sm1: V3 = structMutate.getOr(1, new V3(0, 0, 0));
  const sm2: V3 = structMutate.getOr(2, new V3(0, 0, 0));
  print(`struct mutate ${sm1.x},${sm1.y},${sm1.z}|${sm2.x},${sm2.y},${sm2.z}|`);

  const structOverwrite: Map<i32, V3> = new Map<i32, V3>();
  structOverwrite.set(1, new V3(10, 20, 30));
  structOverwrite.set(2, new V3(40, 50, 60));
  structOverwrite.forEach((value: V3, key: i32): void => {
    structOverwrite.set(key, new V3(700 + key, 800 + key, 900 + key));
    structVisit += `${key}:${value.x},${value.y},${value.z}|`;
  });
  const so1: V3 = structOverwrite.getOr(1, new V3(0, 0, 0));
  const so2: V3 = structOverwrite.getOr(2, new V3(0, 0, 0));
  print(`struct overwrite ${structVisit}${so1.x},${so1.y},${so1.z}|${so2.x},${so2.y},${so2.z}|`);

  const arrayMutate: Map<i32, FixedArray<i32, 3>> =
    new Map<i32, FixedArray<i32, 3>>();
  arrayMutate.set(1, [1, 2, 3]);
  arrayMutate.set(2, [4, 5, 6]);
  arrayMutate.forEach(mutateArray);
  const am1: FixedArray<i32, 3> = arrayMutate.getOr(1, [0, 0, 0]);
  const am2: FixedArray<i32, 3> = arrayMutate.getOr(2, [0, 0, 0]);
  print(`array mutate ${am1[0]},${am1[1]},${am1[2]}|${am2[0]},${am2[1]},${am2[2]}|`);

  const arrayOverwrite: Map<i32, FixedArray<i32, 3>> =
    new Map<i32, FixedArray<i32, 3>>();
  arrayOverwrite.set(1, [10, 20, 30]);
  arrayOverwrite.set(2, [40, 50, 60]);
  arrayOverwrite.forEach((value: FixedArray<i32, 3>, key: i32): void => {
    arrayOverwrite.set(key, [700 + key, 800 + key, 900 + key]);
    arrayVisit += `${key}:${value[0]},${value[1]},${value[2]}|`;
  });
  const ao1: FixedArray<i32, 3> = arrayOverwrite.getOr(1, [0, 0, 0]);
  const ao2: FixedArray<i32, 3> = arrayOverwrite.getOr(2, [0, 0, 0]);
  print(`array overwrite ${arrayVisit}${ao1[0]},${ao1[1]},${ao1[2]}|${ao2[0]},${ao2[1]},${ao2[2]}|`);
}
