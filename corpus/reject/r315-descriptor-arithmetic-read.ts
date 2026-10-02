// corpus: reject/r315-descriptor-arithmetic-read
// purpose: Rejects this in a descriptor member default.
// exercises: descriptor, field-initializer, this-binding
// questions: compiler.md §147, collisions.md C9
// tsc: rejects TS2532
// expected-error: §147 rule 3a
@Descriptor class D { w?: i32 = 800; h?: i32 = this.w * 3 / 4; }
export function main(): void { const d: D = {}; }
