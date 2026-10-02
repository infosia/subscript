// corpus: accept/a324-generic-inference/lib
// purpose: Supplies an imported generic function.
// exercises: generic-function, namespace-import
// questions: compiler.md §149
// tsc: accepts; js-comparable: yes
export function id<T>(x: T): T { return x; }
