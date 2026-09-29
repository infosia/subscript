// corpus: accept/a286-mirror-type-alias-shadow/lib
// purpose: A module class hides a mirror alias only in its own scope.
// exercises: module-import, declaration-identity, mirror-type-alias
// questions: compiler.md §125
// tsc: accepts; js-comparable: yes

export function libRead(): i32 {
  const result: i32 = 100;
  const callback: SubLogCallback = (message, first, second) => {};
  callback("test", null, null);
  return result;
}
