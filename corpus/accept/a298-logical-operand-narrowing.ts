// corpus: accept/a298-logical-operand-narrowing
// purpose: The right operand of a logical operator sees the narrowing of the left operand.
// observable: each narrowed read gives its value; each null path takes the other branch.
// exercises: logical-and-narrowing, logical-or-narrowing, nested-and-chain, shared-field-path, local-path, catch-instanceof-operand, or-else-branch, or-ternary
// questions: compiler.md §133, §124, §46, C6, C7
// tsc: accepts; js-comparable: yes
class Leaf {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

class Node {
  c: Leaf | null;
  constructor(c: Leaf | null) {
    this.c = c;
  }
}

class Tree {
  n: Node | null;
  constructor(n: Node | null) {
    this.n = n;
  }
}

function parameterAnd(hn: Node | null): i32 {
  if (hn !== null && hn.c !== null) {
    return hn.c.v;
  }
  return -1;
}

function parameterValue(hn: Node | null): boolean {
  const b: boolean = hn !== null && hn.c !== null && hn.c.v > 5;
  return b;
}

function parameterOr(hn: Node | null): boolean {
  const b: boolean = hn === null || hn.c === null || hn.c.v < 0;
  return b;
}

function orFalseFacts(hn: Node | null): i32 {
  if (hn === null || hn.c === null) {
    return -2;
  }
  return hn.c.v;
}

function nestedChain(t: Tree | null): i32 {
  if (t !== null && t.n !== null && t.n.c !== null && t.n.c.v > 0) {
    return t.n.c.v;
  }
  return 0;
}

function sharedField(h: Node): boolean {
  const b: boolean = h.c !== null && h.c.v === 7;
  return b;
}

function caughtRange(n: i32): boolean {
  try {
    if (n > 0) {
      throw new RangeError("out of range");
    }
    throw new TypeError("wrong type");
  } catch (e) {
    const b: boolean = e instanceof RangeError && e.message.length > 0;
    return b;
  }
}

function orElse(a: Leaf | null, b: Leaf | null): i32 {
  if (a === null || b === null) {
    return -1;
  } else {
    return a.v + b.v;
  }
}

function orTernary(a: Leaf | null, b: Leaf | null): i32 {
  return a === null || b === null ? -1 : a.v + b.v;
}

export function main(): void {
  const full: Node | null = new Node(new Leaf(7));
  const empty: Node | null = new Node(null);
  print(`parameterAnd:${parameterAnd(full)}:${parameterAnd(empty)}:${parameterAnd(null)}`);
  print(`parameterValue:${parameterValue(full)}:${parameterValue(empty)}:${parameterValue(null)}`);
  print(`parameterOr:${parameterOr(full)}:${parameterOr(empty)}:${parameterOr(null)}`);
  print(`orFalseFacts:${orFalseFacts(full)}:${orFalseFacts(empty)}:${orFalseFacts(null)}`);
  print(`nestedChain:${nestedChain(new Tree(full))}:${nestedChain(new Tree(empty))}:${nestedChain(new Tree(null))}:${nestedChain(null)}`);
  print(`sharedField:${sharedField(new Node(new Leaf(7)))}:${sharedField(new Node(new Leaf(3)))}:${sharedField(new Node(null))}`);

  const l: Node | null = full;
  if (l !== null && l.c !== null) {
    print(`localAnd:${l.c.v}`);
  }
  const x: Leaf | null = new Leaf(4);
  const localPath: boolean = x !== null && x.v === 4;
  print(`localPath:${localPath}`);
  const orEqual: boolean = l === null || l.c === null;
  const orNotEqual: boolean = l === null || l.c !== null;
  print(`localOr:${orEqual}:${orNotEqual}`);
  const m: Node | null = empty;
  const orEmpty: boolean = m === null || m.c === null || m.c.v > 100;
  print(`localOrEmpty:${orEmpty}`);
  const ternary: i32 = l !== null ? (l.c !== null ? l.c.v : 0) : 0;
  print(`ternary:${ternary}`);
  print(`caughtRange:${caughtRange(1)}:${caughtRange(0)}`);
  print(`orElse:${orElse(new Leaf(2), new Leaf(3))}:${orElse(null, new Leaf(3))}:${orElse(new Leaf(2), null)}`);
  print(`orTernary:${orTernary(new Leaf(2), new Leaf(3))}:${orTernary(null, new Leaf(3))}:${orTernary(new Leaf(2), null)}`);
}
