// corpus: reject/r237-catch-annotation
// purpose: Rejects a catch binding annotation other than `unknown`.
// exercises: try-catch, catch-binding, type-annotation, exception
// questions: Q9
// tsc: accepts
// expected-error: S010 at the catch binding
export function main(): void {
  try {
    throw new Error("failure");
  } catch (e: any) {
    print("handler");
  }
}
