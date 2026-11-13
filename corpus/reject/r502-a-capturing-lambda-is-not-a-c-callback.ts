// corpus: reject/r502-a-capturing-lambda-is-not-a-c-callback
// purpose: A C callback cannot retain a capture.
// exercises: lambda, C-callback
// questions: compiler.md §274 rule 5, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 10

export function main(): void {
    const value: i32 = 7;
    new SubCallbackInfo((message, userdata, userparam): void => { print(`${value}`); }, null, null);
}
