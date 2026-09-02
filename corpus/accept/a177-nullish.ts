// corpus: accept/a177-nullish
// purpose: Runs nullish coalescing and optional chains without binding undefined.
// observable: Values and call counts pin short-circuit evaluation and single evaluation.
// exercises: nullish-coalescing, optional-chain, conditional-rewrite, synthetic-local, for-condition, for-initializer, arrow-body
// questions: R39.5, §82.3, §82.10, C7
// tsc: accepts; js-comparable: yes

class Box {
  static touches: i32 = 0;
  v: i32;
  label: string;
  next: Box | null;

  constructor(v: i32, label: string, next: Box | null) {
    this.v = v;
    this.label = label;
    this.next = next;
  }

  choose(keep: boolean): Box | null {
    return keep ? this : null;
  }

  touch(): void {
    Box.touches++;
  }
}

class Source {
  static calls: i32 = 0;

  static get(value: Box | null): Box | null {
    Source.calls++;
    return value;
  }
}

class Fallback {
  static calls: i32 = 0;

  static get(value: Box): Box {
    Fallback.calls++;
    return value;
  }
}

function noBox(): Box | null {
  return null;
}

function maybe(keep: boolean): Box | null {
  return keep ? new Box(1, "maybe", null) : null;
}

export function main(): void {
  const tail: Box = new Box(7, "tail", null);
  const head: Box = new Box(3, "head", tail);
  const present: Box | null = head;
  const absent: Box | null = noBox();
  const nullableTail: Box | null = tail;

  const refRight: Box = present ?? new Box(0, "bad", null);
  print(`ref:${refRight.v}`);

  const nullableRight: Box | null = absent ?? nullableTail;
  print(`nullable:${nullableRight !== null ? nullableRight.v : 0}`);

  const third: Box = absent ?? nullableRight ?? head;
  print(`triple:${third.v}`);

  const kept: Box = Source.get(present) ?? Fallback.get(tail);
  print(`kept:${kept.v}:${Source.calls}:${Fallback.calls}`);

  const replaced: Box = Source.get(absent) ?? Fallback.get(tail);
  print(`replaced:${replaced.v}:${Source.calls}:${Fallback.calls}`);

  const field: Box = present?.next ?? head;
  print(`field:${field.v}`);

  const method: Box = present?.choose(false) ?? tail;
  print(`method:${method.v}`);

  const nested: i32 = present?.next?.v ?? 0;
  print(`nested:${nested}`);

  const numeric: i32 = present?.v ?? 0;
  print(`numeric:${numeric}`);

  const text: string = present?.label.toUpperCase() ?? "";
  print(`string:${text}`);

  absent?.touch();
  print(`statement-null:${Box.touches}`);

  present?.touch();
  print(`statement-value:${Box.touches}`);

  Source.get(present)?.touch();
  print(`statement-call:${Source.calls}:${Box.touches}`);

  for (let i: i32 = 0; i < 2; present?.touch()) {
    i++;
  }
  print(`update:${Box.touches}`);

  const fb: Box = new Box(1, "fallback", null);
  let nullishCount: i32 = 0;
  for (let i: i32 = 0; i < 3 && (maybe(true) ?? fb).v > 0; i++) {
    nullishCount++;
  }
  print(`for-nullish:${nullishCount}`);

  let optionalCount: i32 = 0;
  for (let i: i32 = 0; i < 3 && (maybe(true)?.v ?? 0) > 0; i++) {
    optionalCount++;
  }
  print(`for-optional:${optionalCount}:${fb.v}`);

  let i: i32 = 0;
  let initializerCount: i32 = 0;
  for (i = (maybe(true) ?? fb).v; i < 3; i++) {
    initializerCount++;
  }
  print(`for-initializer:${initializerCount}`);

  const n: i32 = (maybe(true) ?? fb).v + ((): i32 => 2)();
  print(`arrow-owner:${n}`);
}
