// corpus: accept/a329-initializer-inference/main
// purpose: Uses inferred declaration types at each initializer position.
// exercises: module-global, field, static-field, default-parameter, lambda, value-type
// questions: compiler.md §156, C4
// tsc: accepts; js-comparable: yes

import { imported, ValueType } from "./lib";

const MAX = 10;
let count = 0;
const RATE = 0.5;
const NAME = "pool";
const ids = [1, 2, 3];
const twice = (x: i32): i32 => { return x * 2; };
const readLater = () => later;
const later = 1;
const countdown: (n: i32) => i32 =
    (n: i32): i32 => n === 0 ? 0 : countdown(n - 1);
const read = () => value();
const value: () => i32 = (): i32 => read();

class Pool {
    size = 4;
    label = "c";
    ratio = 1.5;
    static limit = 8;
}

@ValueType
class Point {
    x = 3;
}

function grow(n: i32, by = 2): i32 { return n + by; }

function first(n = second(1)): i32 { return n; }
function second(n = first(1)): i32 { return n; }
function annotatedFirst(n: i32 = annotatedSecond(1)): i32 { return n; }
function annotatedSecond(n: i32 = annotatedFirst(1)): i32 { return n; }

export function host(by = 2): void { print(`${by}`); }

const sliced = [1, 2, 3].slice(1);
const joined = ["a", "b"].join("+");
const indexed = [1, 2, 3].indexOf(2);
const included = [1, 2, 3].includes(2);
const fixed = (1.5).toFixed(2);
const textSlice = "hello".slice(1, 3);
const total = sum(1);
function sum(base = total): i32 { return base; }

class Builtins {
    sliced = [1, 2, 3].slice(1);
    joined = ["a", "b"].join("+");
    indexed = [1, 2, 3].indexOf(2);
    included = [1, 2, 3].includes(2);
    fixed = (1.5).toFixed(2);
    textSlice = "hello".slice(1, 3);
    static sliced = [1, 2, 3].slice(1);
    static joined = ["a", "b"].join("+");
    static indexed = [1, 2, 3].indexOf(2);
    static included = [1, 2, 3].includes(2);
    static fixed = (1.5).toFixed(2);
    static textSlice = "hello".slice(1, 3);
    step = 3;
    m(by = this.step): i32 { return by; }
}
function builtinDefaults(a = [1, 2, 3].slice(1), j = ["a", "b"].join("+"),
    i = [1, 2, 3].indexOf(2), b = [1, 2, 3].includes(2),
    f = (1.5).toFixed(2), t = "hello".slice(1, 3)): void {
    print(`${a.length},${j},${i},${b},${f},${t}`);
}
function make<T>(n: i32): T[] { const arr: T[] = []; return arr; }
class Box<T> { store = make<T>(1); }
function fill<T>(v: T, arr = make<T>(0)): i32 { return arr.length; }
function depthA(n: i32, acc = n > 0 ? depthB(n - 1) : 0): i32 { return n + acc; }
function depthB(n: i32, acc = n > 0 ? depthA(n - 1) : 0): i32 { return n + acc; }
function typedDepthA(n: i32, acc: i32 = n > 0 ? typedDepthB(n - 1) : 0): i32 { return n + acc; }
function typedDepthB(n: i32, acc: i32 = n > 0 ? typedDepthA(n - 1) : 0): i32 { return n + acc; }
function rec(n: i32, k = (m: i32): i32 => m === 0 ? 0 : rec(m - 1)): i32 { return k(n); }
function shadow(n = ((n: i32) => n + 1)(1)): i32 { return n; }
function localShadow(n = ((): i32 => { const n = 2; return n; })()): i32 { return n; }
function mapShadow(list = [5].map((list) => list)): i32 { return list[0]; }

function earlier(a: i32, b = 2, c = b + 1): i32 { return a + c; }
function range(x = 1, y = x * 2): i32 { return y; }
const addDefaults = (a = 1, b = a * 2): i32 => a + b;
class PriorDefaults {
    v = 3;
    total: i32;
    constructor(x = 1, y = x * 2) { this.total = y; }
    pick(alt = this.v, list = [alt, this.v]): i32 { return list[0] + list[1]; }
}
class Tree {
    child: Tree | null;
    constructor(n: i32, child = n > 0 ? new Tree(n - 1) : null) { this.child = child; }
}
function direct(n: i32, k = direct(n - 1, 0)): i32 { return n + k; }
function omitted(n: i32, k = n > 0 ? omitted(n - 1) : 0): i32 { return n + k; }
function pairedFirst(n = pairedSecond(1) + 0): i32 { return n; }
function pairedSecond(n = pairedFirst(1)): i32 { return n; }
class DirectMethod {
    m(n: i32, k = this.m(n - 1, 0)): i32 { return n + k; }
}
class S {
    base: i32 = 7;
    async m(n: i32 = this.base, k = n + 2): Promise<i32> { return n + k; }
    static *numbers(n = 7, k = n + 2): Generator<i32> { yield n + k; }
}
async function receiverDefaults(): Promise<void> { print(`${await new S().m()}`); }

export async function main(): Promise<void> {
    const pool = new Pool();
    const point = new Point();
    count = grow(count);
    print(`${MAX},${count},${RATE},${NAME},${ids[2]},${twice(3)},${imported}`);
    print(`${pool.size},${pool.label},${pool.ratio},${Pool.limit},${point.x}`);
    print(`${countdown(3)},${readLater()}`);
    print(`${first()},${annotatedFirst()}`);
    print(`${sliced.length},${joined},${indexed},${included},${fixed},${textSlice}`);
    const builtins = new Builtins();
    print(`${builtins.sliced.length},${builtins.joined},${builtins.indexed},${builtins.included},${builtins.fixed},${builtins.textSlice}`);
    print(`${Builtins.sliced.length},${Builtins.joined},${Builtins.indexed},${Builtins.included},${Builtins.fixed},${Builtins.textSlice}`);
    builtinDefaults();
    const box = new Box<f64>();
    print(`${box.store.length},${fill("a")}`);
    print(`${depthA(6) + 1},${typedDepthA(6) + 1},${rec(3)},${builtins.m()}`);
    print(`${shadow()},${localShadow()},${mapShadow()},${total},${sum()}`);
    const prior = new PriorDefaults();
    print(`${earlier(1)},${range()},${addDefaults(1, 2)},${prior.total},${prior.pick()}`);
    print(`${new Tree(2).child !== null},${direct(2)},${omitted(2)},${pairedFirst()},${pairedSecond()},${new DirectMethod().m(2)}`);
    for (const n of S.numbers()) { print(`${n}`); }
    await receiverDefaults();

}
