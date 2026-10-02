// corpus: accept/a326-postfix-update
// purpose: A postfix update yields the old value and evaluates its target once.
// exercises: postfix-update, prefix-update, captured-local, module-global
// questions: compiler.md §150
// tsc: accepts; js-comparable: yes
let global: i32 = 20;
let calls: i32 = 0;
function next(): i32 { calls++; return 0; }
class Box { x: i32 = 0; }
export function main(): void {
  let k: i32 = 0;
  const a = k++;
  print(`${a} ${k}`);
  let j: i32 = 5;
  const b = j--;
  print(`${b} ${j}`);
  const o = new Box();
  const c = o.x++;
  print(`${c} ${o.x}`);
  const xs: i32[] = [10];
  const e = xs[0]++;
  print(`${e} ${xs[0]}`);
  let i: i32 = 0;
  let n: i32 = 0;
  while (i++ < 3) { n = n + 1; }
  print(`${n} ${i}`);
  let q: f64 = 1.5;
  const g = q++;
  print(`${g} ${q}`);
  let p: i32 = 0;
  const f = ++p;
  print(`${f} ${p}`);
  const h = global++;
  print(`${h} ${global}`);
  const captured = new Box();
  captured.x = 30;
  const update = (): i32 => captured.x++;
  const oldCaptured = update();
  print(`${oldCaptured} ${captured.x}`);
  let byte: u8 = 255;
  const oldByte = byte++;
  // The explicit mask also makes the Node witness use the target width.
  byte = byte & 255;
  print(`${oldByte} ${byte}`);
  let wide: i64 = 40;
  const oldWide = wide++;
  print(`${oldWide} ${wide}`);
  const oldIndex = xs[next()]++;
  print(`${oldIndex} ${xs[0]} ${calls}`);
}
