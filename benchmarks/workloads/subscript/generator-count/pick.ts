function* gen(): Generator<i32> { yield 1; }
function pick(x: Generator<i32>): Generator<i32> { return x; }
export function main(): void {
 const g = gen();
 let s: i32 = 0;
 for (let i: i32 = 0; i < 10000000; i++) { pick(g); s += 1; }
 print(`${s}`);
}
