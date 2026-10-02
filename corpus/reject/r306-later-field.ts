// corpus: reject/r306-later-field
// purpose: Rejects a forbidden this form in a field initializer.
// exercises: field-initializer, this-binding
// questions: §147 rule 2, collisions.md C9
// tsc: rejects TS2729
// expected-error: §147 rule 2
class A { y: i32 = this.x; x: i32 = 3; }
export function main(): void {}
