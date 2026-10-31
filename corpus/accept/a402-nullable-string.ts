// corpus: accept/a402-nullable-string
// purpose: A nullable string is a `Ref | null` form in each position, and its equality and array search compare by content in both operand orders.
// exercises: nullable-string, nullable-parameter, nullable-field, null-narrowing, nullish-coalescing, nullish-assignment, optional-chain, generic-class, generic-function, Map, array-search, json, Context.collect
// questions: compiler.md §218, C7
// tsc: accepts; js-comparable: yes
class Holder {
  name: string | null = null;
  note: string | null = "x\"y";
  n: i32 = 3;
}

class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}

let shared: string | null = null;

function built(a: string, b: string): string {
  return a + b;
}

function maybe(n: i32): string | null {
  if (n < 0) {
    return null;
  }
  return `v${n}`;
}

function sizeOf(value: string | null): i32 {
  if (value === null) {
    return -1;
  }
  return value.length;
}

function orElse(value: string | null, fallback: string): string {
  return value ?? fallback;
}

function same(a: string | null, b: string | null): boolean {
  return a === b;
}

function differ(a: string | null, b: string | null): boolean {
  return a !== b;
}

function sameAny<T>(a: T, b: T): boolean {
  return a === b;
}

function mixed(a: string | null, s: string): string {
  return `${a === s} ${s === a} ${a !== s} ${s !== a} ${a == s} ${s != a} ${a === "abc"} ${"abc" === a}`;
}

function lookup(n: i32): string | undefined {
  if (n < 0) {
    return undefined;
  }
  return `u${n}`;
}

function append(acc: string | null, s: string): string | null {
  return acc === null ? s : acc + s;
}

function firstLength(acc: Holder | null, s: string): Holder | null {
  if (acc !== null) {
    return acc;
  }
  const made = new Holder();
  made.n = s.length;
  return made;
}

function lengths(acc: i32[] | null, s: string): i32[] | null {
  const out: i32[] = acc ?? [];
  out.push(s.length);
  return out;
}

function fillShared(): void {
  shared = built("sev", "en");
}

export function main(): void {
  // A parameter, a result, and a local.
  print(`${sizeOf(maybe(1))} ${sizeOf(maybe(-1))} ${sizeOf(null)} ${sizeOf("abc")}`);
  let local: string | null = null;
  print(`${local === null} ${local == null}`);
  local = "q";
  print(`${local} ${local !== null}`);

  // Equality by content, in both operand orders.
  const x: string | null = built("ab", "c");
  const y: string | null = built("a", "bc");
  const none: string | null = maybe(-1);
  print(`${same(x, y)} ${same(y, x)} ${differ(x, y)}`);
  print(`${same(x, null)} ${same(null, x)} ${same(null, null)} ${same(none, none)}`);
  print(`${differ(x, null)} ${differ(null, x)} ${differ(null, null)}`);
  print(`${same(x, "abc")} ${same("abc", x)} ${same(x, "abd")}`);
  print(mixed(built("ab", "c"), built("a", "bc")));
  print(mixed(null, "abc"));
  print(mixed("", ""));
  print(`${sameAny<string | null>(x, y)} ${sameAny<string | null>(null, y)} ${sameAny<string | null>(y, null)}`);

  // An empty string is not absent.
  const empty: string | null = built("", "");
  print(`${empty === null} ${same(empty, null)} ${same(null, empty)} ${same(empty, "")}`);
  print(`[${empty ?? "fallback"}] [${orElse(null, "fallback")}]`);

  // A field, and a collection with a null field.
  const holder = new Holder();
  const filled = new Holder();
  filled.name = built("fi", "lled");
  Context.collect();
  print(`${holder.name === null} ${sizeOf(filled.name)}`);
  print(JSON.stringify(holder));
  print(JSON.stringify(filled));
  const parsed = JSON.parse<Holder>("{\"name\":null,\"note\":\"hey\",\"n\":4}");
  print(`${parsed.name === null} ${parsed.note ?? "none"} ${parsed.n}`);

  // An array element, a search by content, and a Map value.
  const rows: (string | null)[] = ["ab", null, "cd"];
  print(`${rows.includes(built("a", "b"))} ${rows.includes(null)} ${rows.includes("zz")}`);
  print(`${rows.indexOf(built("c", "d"))} ${rows.indexOf(null)} ${rows.lastIndexOf(null)} ${rows.lastIndexOf(built("a", "b"))}`);
  let total = 0;
  for (const row of rows) {
    total += row?.length ?? 100;
  }
  print(`${total} ${rows[1] === null}`);
  rows.push(null);
  rows[0] = built("n", "ew");
  Context.collect();
  print(`${rows.length} ${rows[0] ?? "none"} ${rows[3] === null}`);
  const table = new Map<string, string | null>();
  table.set("a", built("v", "1"));
  table.set("b", null);
  print(`${table.size} ${table.has("b")}`);

  // A global.
  print(`${shared === null}`);
  fillShared();
  print(`${sizeOf(shared)} ${same(shared, "seven")}`);
  shared = null;
  print(`${shared === null}`);

  // A generic argument.
  const blank = new Box<string | null>(null);
  const full = new Box<string | null>(built("b", "ox"));
  print(`${blank.value === null} ${full.value ?? "none"}`);

  // `??`, `??=`, and `?.` with a member and a call.
  let pending: string | null = null;
  pending ??= "five";
  print(pending);
  const upper = maybe(2)?.toUpperCase() ?? "none";
  print(`${upper} ${maybe(-1)?.length ?? -1}`);

  // A `reduce` seed takes the accumulator type.
  const words: string[] = ["a", "bb"];
  print(words.reduce(append, built("se", "ed")) ?? "none");
  print(words.reduce(append, null) ?? "none");
  print(words.reduceRight(append, "seed") ?? "none");
  print(words.reduceRight(append, null) ?? "none");
  const fresh = new Holder();
  fresh.n = 7;
  print(`${words.reduce(firstLength, fresh)?.n ?? -1} ${words.reduce(firstLength, null)?.n ?? -1}`);
  print(`${words.reduceRight(firstLength, fresh)?.n ?? -1} ${words.reduceRight(firstLength, null)?.n ?? -1}`);
  print(`${(words.reduce(lengths, [9]) ?? []).length} ${(words.reduce(lengths, null) ?? []).length}`);
  print(`${(words.reduceRight(lengths, [9]) ?? []).length} ${(words.reduceRight(lengths, null) ?? []).length}`);

  // A `string | undefined` result compares by content and with `== null`.
  print(`${lookup(-1) == null} ${lookup(1) == null} ${lookup(-1) === undefined} ${lookup(1) !== undefined}`);
  print(`${lookup(1) === built("u", "1")} ${built("u", "1") === lookup(1)} ${lookup(1) !== "u2"} ${lookup(-1) === "u1"}`);
  print(`${lookup(1) === lookup(1)} ${lookup(-1) === lookup(-1)} ${lookup(1) === lookup(-1)}`);
}
