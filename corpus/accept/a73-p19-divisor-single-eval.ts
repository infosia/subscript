// corpus: accept/a73-p19-divisor-single-eval
export function main(): void {
  const q: i32 = 100 / ((Math.random() * 3.0 + 1.0) as i32);
  print(`q ${q}`);
  print(`next ${Math.random()}`);
}
