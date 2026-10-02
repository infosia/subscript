// corpus: reject/r313-descriptor-later-field
// purpose: Rejects this in a descriptor member default.
// exercises: descriptor, field-initializer, this-binding
// questions: compiler.md §147, collisions.md C9
// tsc: accepts
// expected-error: §147 rule 3a
@Descriptor class D { a?: i32 = this.b; b?: i32 = 7; }
export function main(): void { const d: D = {}; }
