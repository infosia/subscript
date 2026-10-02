// corpus: reject/r311-this-value-field-initializer
// purpose: Rejects a forbidden this form in a field initializer.
// exercises: field-initializer, this-binding
// questions: §147 rule 2, collisions.md C9
// tsc: accepts
// expected-error: §147 rule 2
class A { y: A | null = this; }
export function main(): void {}
