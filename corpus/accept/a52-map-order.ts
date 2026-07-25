// corpus: accept/a52-map-order
// purpose: Pins Map insertion order across insert, overwrite without
//          movement, delete, and re-insertion at the end.
// exercises: map-insertion-order, overwrite, delete-reinsert
// questions: Q24

let order: string = "";

export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(2, "b");
  map.set(1, "a");
  map.set(3, "c");
  map.set(2, "B");
  map.delete(1);
  map.set(1, "A");
  map.forEach((value: string, key: i32): void => {
    order += `${key}:${value}|`;
  });
  print(order);
}
