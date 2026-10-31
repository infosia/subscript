// corpus: accept/a411-element-receiver-cycle
// purpose: A traversal of a cycle of array elements ends, because this and the element name one object, and a push to an array of another element type moves no element.
// exercises: value-struct, dynamic-array, method-receiver, recursion
// questions: compiler.md §221.1 rule 8
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class Node {
  id: i32;
  next: i32;
  seen: boolean = false;

  constructor(id: i32, next: i32) {
    this.id = id;
    this.next = next;
  }

  visit(): void {
    if (this.seen) {
      return;
    }
    this.seen = true;
    order.push(this.id);
    nodes[this.next].visit();
  }
}

const nodes: Node[] = [new Node(1, 1), new Node(2, 2), new Node(3, 0)];
const order: i32[] = [];

export function main(): void {
  nodes[0].visit();
  print(`count ${order.length} ${order.join(" ")}`);
}
