// corpus: reject/r312-lambda-field-initializer
// purpose: Rejects a forbidden this form in a field initializer.
// exercises: field-initializer, this-binding
// questions: §147 rule 2, collisions.md C9
// tsc: accepts
// expected-error: §147 rule 2
class A { x: i32 = 3; y: () => i32 = () => this.x; }
export function main(): void {}
