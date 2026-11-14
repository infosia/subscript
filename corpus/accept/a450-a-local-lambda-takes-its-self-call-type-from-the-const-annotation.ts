// corpus: accept/a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation
// purpose: A const annotation gives a local lambda its self-call type.
// observable: Block, expression, async, and string results use the annotated type.
// exercises: lambda, recursion, contextual-typing, type-alias, async-arrow, await, string
// questions: compiler.md §285
// tsc: accepts; js-comparable: yes
type Factorial = (k: i32) => i32;
export async function main(): Promise<void> {
  const fact: Factorial = (k) => {
    if (k <= 1) return 1;
    return k * fact(k - 1);
  };
  const expression: (k: i32) => i32 = (k) => k <= 1 ? 1 : k * expression(k - 1);
  const asynchronous: (k: i32) => Promise<i32> = async (k) => {
    if (k <= 1) return 1;
    return k * await asynchronous(k - 1);
  };
  const text: (k: i32) => string = (k) => k <= 0 ? "end" : `step:${text(k - 1)}`;
  print(`${fact(5)}`);
  print(`${expression(5)}`);
  print(`${await asynchronous(5)}`);
  print(text(3));
}
