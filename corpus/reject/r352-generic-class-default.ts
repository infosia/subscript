// corpus: reject/r352-generic-class-default
// purpose: Rejects a generic-class method default that contains a class type parameter.
// exercises: generic-class, method, default-parameter
// expected-error: S100 at line 11, Annotate the generic-class method parameter.
// tsc: accepts
// questions: compiler.md §156 rule 6, C24 row 4

class Cell<T> {
    v: T;
    constructor(v: T) { this.v = v; }
    pair(other = this.v): string { return `${other}`; }
}
export function main(): void {}
