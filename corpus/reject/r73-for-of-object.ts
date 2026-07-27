// corpus: reject/r73-for-of-object
// purpose: Rejects for-of over the opaque object type.
// exercises: for-of-closed-list
// questions: Q30
// expected-error: object is not one of the built-in iterable containers

class Box {
  value: i32;
}

export function visit(source: Box): void {
  for (const value of (source as object)) {
    print(`${value}`);
  }
}

export function main(): void {
}
