// corpus: accept/a67-q27-array-callback-index
// purpose: Exercises both accepted callback arities for every Q27
//          indexed Array method and makes each index affect output.
//          reduceRight prints the indices in callback visit order.
// exercises: array-callback-index, dual-callback-arity, runtime-to-script-calls
// questions: Q27, C5
// tsc: accepts; js-comparable: yes
let valueTotal: i32 = 0;
let indexedTrace: string = "";

function addIndex(value: i32, index: i32): i32 {
  return value + index;
}

export function main(): void {
  const values: i32[] = [4, 7, 10, 13];

  valueTotal = 0;
  values.forEach((value: i32): void => {
    valueTotal += value;
  });
  indexedTrace = "";
  values.forEach((value: i32, index: i32): void => {
    indexedTrace += `${index}:${value}|`;
  });
  print(`forEach ${valueTotal} ${indexedTrace}`);

  const mappedValue: i32[] = values.map((value: i32): i32 => value * 2);
  const mappedIndex: i32[] = values.map(addIndex);
  print(`map ${mappedValue.join(",")} ${mappedIndex.join(",")}`);

  const filteredValue: i32[] = values.filter((value: i32): boolean => value % 2 === 0);
  const filteredIndex: i32[] = values.filter(
    (value: i32, index: i32): boolean => value > 0 && index % 2 === 1,
  );
  print(`filter ${filteredValue.join(",")} ${filteredIndex.join(",")}`);

  const someValue: boolean = values.some((value: i32): boolean => value > 12);
  const someIndex: boolean = values.some(
    (value: i32, index: i32): boolean => index === 2 && value === 10,
  );
  print(`some ${someValue} ${someIndex}`);

  const everyValue: boolean = values.every((value: i32): boolean => value > 0);
  const everyIndex: boolean = values.every(
    (value: i32, index: i32): boolean => value === 4 + index * 3,
  );
  print(`every ${everyValue} ${everyIndex}`);

  const foundValue: i32 = values.findIndex((value: i32): boolean => value === 10);
  const foundIndex: i32 = values.findIndex(
    (value: i32, index: i32): boolean => index === 3 && value === 13,
  );
  print(`findIndex ${foundValue} ${foundIndex}`);

  const weighted: i32 = values.reduce(
    (acc: i32, value: i32, index: i32): i32 => acc + value * index,
    0,
  );
  print(`reduce ${weighted}`);

  const rightIndices: string = values.reduceRight(
    (acc: string, value: i32, index: i32): string =>
      acc + (value > 0 ? (acc.length === 0 ? `${index}` : `,${index}`) : ""),
    "",
  );
  print(`reduceRight ${rightIndices}`);
}
