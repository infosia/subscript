// corpus: accept/a474-c-array-parameter-storage
// interpreter: no — calls the synthetic native interop library
// purpose: C array parameters read and write the argument storage (compiler.md §324).
// exercises: fixed-array-parameter, const-array, scratch-array, field-address, element-address, temporary-address, alias-write-back
// questions: Q13
// tsc: accepts
// js-comparable: no Q13: The C boundary has no JavaScript shim.

class FixedBox {
  values: FixedArray<i32, 4> = [1, 2, 3, 4];
}
function fixedResult(): FixedArray<i32, 4> { return [1, 2, 3, 4]; }
function ints(): void {
  const values: FixedArray<i32, 4> = [1, 2, 3, 4];
  subFixedInts(values);
  print(`${values[0]} ${values[1]} ${values[2]} ${values[3]}`);
}
function readInts(): void {
  const values: FixedArray<i32, 4> = [2, 4, 6, 8];
  print(`${subFixedReadInts(values)} ${values[0]} ${values[1]} ${values[2]} ${values[3]}`);
}
function bytes(): void {
  const values: FixedArray<SubByValueI32One, 2> = [new SubByValueI32One(1), new SubByValueI32One(2)];
  subFixedBytes(values);
  print(`${values[0].a} ${values[1].a}`);
}
function lays(): void {
  const values: FixedArray<SubDescReadLay, 2> = [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])];
  subFixedLays(values);
  print(`${values[0].tag} ${values[1].tag} ${values[0].items[0]} ${values[1].items[0]}`);
}
function handles(): void {
  const first = subDeviceCreate(null);
  const second = subDeviceCreate(null);
  const values: FixedArray<SubDevice, 2> = [first, second];
  subFixedHandles(values);
  print(`${values[0] === second} ${values[1] === first}`);
  subDeviceRelease(first);
  subDeviceRelease(second);
}
function strings(): void {
  const values: FixedArray<SubDescReadStr, 2> = [new SubDescReadStr("a", 1), new SubDescReadStr("b", 2)];
  subFixedStrings(values);
  Context.collect();
  print(`${values[0].label} ${values[1].label} ${values[0].n} ${values[1].n}`);
}
function field(): void {
  const box = new FixedBox();
  print(`${subFixedSum(box.values)} ${box.values[0]} ${box.values[3]}`);
}
function temporary(): void { print(`${subFixedSum(fixedResult())}`); }
function element(): void {
  const values: FixedArray<i32, 4>[] = [[1, 2, 3, 4]];
  print(`${subFixedSum(values[0])} ${values[0][0]} ${values[0][3]}`);
}
function readLays(): void {
  const values: FixedArray<SubDescReadLay, 2> = [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])];
  print(`${subFixedReadLays(values)} ${values[0].tag} ${values[1].tag}`);
}
function aliases(): void {
  const values: FixedArray<SubDescReadLay, 2> = [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])];
  subFixedTwoLays(values, values);
  print(`${values[0].tag}`);
  subFixedLayPointer(values, values[0]);
  print(`${values[0].tag}`);
}
export function main(): void {
  ints(); readInts(); bytes(); lays(); handles(); strings(); field(); temporary(); element(); readLays(); aliases();
}
