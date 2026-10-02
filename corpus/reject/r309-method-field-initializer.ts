// corpus: reject/r309-method-field-initializer
// purpose: Rejects a forbidden this form in a field initializer.
// exercises: field-initializer, this-binding
// questions: §147 rule 2, collisions.md C9
// tsc: accepts
// expected-error: §147 rule 2
class A { y: i32 = this.f(); f(): i32 { return 3; } }
export function main(): void {}
