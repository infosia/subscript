// corpus: reject/r138-bytes-of-reference-class
// purpose: Rejects storage-byte access for a reference class.
// exercises: Context.bytesOf, reference-class-rejection
// questions: R34

class Node {
  value: i32 = 0;
}

export function main(): void {
  const node: Node = new Node();
  Context.bytesOf<Node>(node);
}
