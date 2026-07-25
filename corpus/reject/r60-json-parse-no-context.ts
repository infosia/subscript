// tsc: pass
// purpose: Rejects JSON.parse when neither a type argument nor a contextual
//          JsonResult<T> supplies the monomorphization target.
// expected: S014 at parse

export function main(): void {
  JSON.parse("{}");
}
