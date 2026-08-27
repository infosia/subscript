// corpus: accept/a17-null-story
// purpose: Narrows nullable parameters and fields before reference use.
// exercises: nullable-parameter, nullable-field, null-narrowing
// questions: Q1, Q2, Q6, Q8, Q12
// tsc: accepts; js-comparable: no Q6: The Context memory API has no JavaScript shim.
class ListNode {
  value: i32;
  next: ListNode | null;

  constructor(value: i32, next: ListNode | null) {
    this.value = value;
    this.next = next;
  }
}

function nextValue(node: ListNode | null): i32 {
  if (node === null) {
    return -1;
  }
  if (node.next === null) {
    return node.value;
  }
  return node.next.value;
}

export function main(): void {
  const tail: ListNode = new ListNode(9, null);
  const head: ListNode = new ListNode(4, tail);
  print(`${nextValue(head)},${nextValue(null)}`);
  Context.free(head);
  Context.free(tail);
}
