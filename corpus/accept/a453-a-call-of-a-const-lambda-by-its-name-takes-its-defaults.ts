// corpus: accept/a453-a-call-of-a-const-lambda-by-its-name-takes-its-defaults
// purpose: A call of a const lambda by its name takes its defaults.
// observable: Defaults run left to right once when omitted, including self-calls.
// exercises: lambda, default-argument, self-call, async-arrow
// questions: compiler.md §288, §284
// tsc: accepts; js-comparable: yes
const log: string[] = [];
const top = (k: i32, s: string = "t"): void => { log.push(`${s}${k}`); };
function counter(): i32 { log.push("default"); return 7; }
export async function main(): Promise<void> {
  top(2);
  const loc = (k: i32, s: string = "d"): void => {
    log.push(`${s}${k}`);
    if (k > 0) loc(k - 1);
  };
  loc(2);
  const earlier = (a: i32, b: i32 = a + 1, c: i32 = b + 1): void => {
    log.push(`${a}:${b}:${c}`);
  };
  earlier(3);
  earlier(3, 8);
  const effect = (n: i32 = counter()): void => { log.push(`${n}`); };
  effect();
  effect(9);
  effect();
  const again = async (k: i32, s: string = "a"): Promise<void> => {
    log.push(`${s}${k}`);
    if (k > 0) await again(k - 1);
  };
  await again(2);
  print(log.join(" "));
}
