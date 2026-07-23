// benchmark: tree
// Build, traverse, and explicitly free 30 full binary trees of depth 16
// (131071 nodes each) using reference classes and unsafeDelete — the
// language's manual-lifetime path (no implicit GC, design invariant 2).
// Checksum: total node-visit count, i64 = 30 * (2^17 - 1) = 3932130.

const DEPTH: i32 = 16;
const COUNT: i32 = 30;

class Node {
  left: Node | null;
  right: Node | null;

  constructor(left: Node | null, right: Node | null) {
    this.left = left;
    this.right = right;
  }
}

function build(depth: i32): Node {
  if (depth === 0) {
    return new Node(null, null);
  }
  return new Node(build(depth - 1), build(depth - 1));
}

function check(node: Node): i32 {
  const left: Node | null = node.left;
  const right: Node | null = node.right;
  if (left === null) {
    return 1;
  }
  if (right === null) {
    return 1;
  }
  return 1 + check(left) + check(right);
}

function free(node: Node): void {
  const left: Node | null = node.left;
  const right: Node | null = node.right;
  if (left !== null) {
    free(left);
  }
  if (right !== null) {
    free(right);
  }
  unsafeDelete(node);
}

export function main(): void {
  let checksum: i64 = 0;
  for (let i: i32 = 0; i < COUNT; i += 1) {
    const root: Node = build(DEPTH);
    checksum += (check(root) as i64);
    free(root);
  }
  print(`${checksum}`);
}
