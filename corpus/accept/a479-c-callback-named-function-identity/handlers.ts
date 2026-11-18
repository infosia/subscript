// corpus: accept/a479-c-callback-named-function-identity/handlers
// purpose: Supplies shared callback functions and userdata.
// exercises: C-callback, named-function, module-export
// questions: compiler.md §328 rule 2
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export class Cookie { value: i32 = 7; }
export function onLog(message: string): void { print(message); }
export function otherLog(message: string): void { print(message); }
export function onIdle(): void {}
