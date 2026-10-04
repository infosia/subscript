// corpus: accept/a329-initializer-inference/lib
// purpose: Exports a module constant with an inferred type.
// exercises: module-import, initializer-inference
// questions: compiler.md §156, C4
// tsc: accepts; js-comparable: yes

export const imported = 7;

// The Node witness supplies its decorator as source, without a runtime shim.
export function ValueType<T>(value: T): void {}
