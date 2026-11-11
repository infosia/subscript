// corpus: reject/r496-a-void-result-place-names-the-handle-it-drops
// purpose: A void result place drops the handle of an async function value; the diagnostic names the handle.
// exercises: function-value, async, void-place
// questions: compiler.md §247.1 rules 1 and 2
// tsc: accepts
// expected-error: S013 at line 21, function value `work` returns `Promise<void>`

type CB = (x: i32) => void;

function each(xs: i32[], f: CB): void {
  for (const x of xs) {
    f(x);
  }
}

async function work(x: i32): Promise<void> {
  print(`${x}`);
}

export function main(): void {
  each([1, 2], work);
}
