// example: e04-null
// teaches: Represent absence as T | null and narrow it with !== null before member access.
// differs-from-typescript: C7 permits only Ref | null, with no undefined and no general unions.
// see: corpus/accept/a17-null-story.ts, corpus/reject/r12-general-union.ts, corpus/reject/r13-undefined.ts, collisions.md C7

class Node {
  active: boolean;
  // C7: Ref | null is the sole in-language union, so null is the only
  // absent case that must be narrowed before member access.
  // Rejected alternative: a general union is S011, "unions are limited to
  // `Ref | null`"; corpus/reject/r12-general-union.ts pins it.
  // Rejected alternative: Node | undefined is S012, "`undefined` is banned;
  // the single null story is `null`"; corpus/reject/r13-undefined.ts pins it.
  next: Node | null;

  constructor(active: boolean, next: Node | null) {
    this.active = active;
    this.next = next;
  }
}

function activeOrFalse(node: Node | null): boolean {
  if (node !== null) {
    return node.active;
  }
  return false;
}

function nextActiveOrFalse(node: Node): boolean {
  if (node.next !== null) {
    return node.next.active;
  }
  return false;
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  const tail: Node = new Node(false, null);
  const head: Node = new Node(true, tail);
  print(
    `values=${activeOrFalse(head)},${activeOrFalse(null)},${nextActiveOrFalse(head)},${nextActiveOrFalse(tail)}`,
  );
  // Q6: these reference-class lifetimes end explicitly.
  Context.free(head);
  Context.free(tail);
}
