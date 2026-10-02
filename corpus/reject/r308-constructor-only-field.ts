// corpus: reject/r308-constructor-only-field
// purpose: Rejects a forbidden this form in a field initializer.
// exercises: field-initializer, this-binding
// questions: §147 rule 2, collisions.md C9
// tsc: rejects TS2729
// expected-error: §147 rule 2
class A { x: i32; y: i32 = this.x; constructor() { this.x = 3; } }
export function main(): void {}
