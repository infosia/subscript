// corpus: reject/r314-descriptor-this-value
// purpose: Rejects this in a descriptor member default.
// exercises: descriptor, field-initializer, this-binding
// questions: compiler.md §147, collisions.md C9
// tsc: accepts
// expected-error: §147 rule 3a
@Descriptor class D { self?: D | null = this; }
export function main(): void { const d: D = {}; }
