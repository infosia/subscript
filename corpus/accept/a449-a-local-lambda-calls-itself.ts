// corpus: accept/a449-a-local-lambda-calls-itself
// purpose: A local lambda calls the running closure with the same environment.
// observable: Each recursive call prints its argument and its capture.
// exercises: lambda, recursion, capture, async-arrow, await, loop
// questions: compiler.md §284
// tsc: accepts; js-comparable: yes
const moduleAnnotated = (k: i32): void => { print(`annotated${k}`); if (k > 0) moduleAnnotated(k - 1); };
const moduleInferred = (k: i32) => { print(`inferred${k}`); if (k > 0) moduleInferred(k - 1); };
const moduleCapturedValue: i32 = 7;
const moduleCaptured = (k: i32): void => { print(`capture${moduleCapturedValue}:${k}`); if (k > 0) moduleCaptured(k - 1); };
const moduleAsync = async (k: i32) => { print(`async${k}`); if (k > 0) await moduleAsync(k - 1); };
const moduleEscaped = (k: i32): void => { print(`escape${moduleCapturedValue}:${k}`); if (k > 0) moduleEscaped(k - 1); };
const moduleLoop = (k: i32): void => { for (let i: i32 = 0; i < k; i++) { print(`loop${k}:${i}`); moduleLoop(k - 1); } };
function make(value: i32): (k: i32) => void {
  const copy = value;
  const escaped = (k: i32): void => { print(`escape${copy}:${k}`); if (k > 0) escaped(k - 1); };
  return escaped;
}
export async function main(): Promise<void> {
  moduleAnnotated(2); moduleInferred(2); moduleCaptured(2);
  await moduleAsync(2); moduleEscaped(2); moduleLoop(2);
  const annotated = (k: i32): void => { print(`annotated${k}`); if (k > 0) annotated(k - 1); };
  const inferred = (k: i32) => { print(`inferred${k}`); if (k > 0) inferred(k - 1); };
  const copy: i32 = 7;
  const captured = (k: i32): void => { print(`capture${copy}:${k}`); if (k > 0) captured(k - 1); };
  const asynchronous = async (k: i32) => { print(`async${k}`); if (k > 0) await asynchronous(k - 1); };
  const loop = (k: i32): void => { for (let i: i32 = 0; i < k; i++) { print(`loop${k}:${i}`); loop(k - 1); } };
  annotated(2); inferred(2); captured(2); await asynchronous(2);
  const escaped = make(7); escaped(2); escaped(1); loop(2);
}
