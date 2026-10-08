// corpus: accept/a352-finalizer-walker-controls/lib
// purpose: Supplies the source decorator for the Node witness.
// exercises: module-import, decorator
// questions: C2
// tsc: accepts; js-comparable: yes
export function ValueType<T>(value: T): void {}
