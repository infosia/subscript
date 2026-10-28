// corpus: accept/a389-do-while-test-narrows-the-body
// purpose: The narrowing that a do…while test gives on true holds in the body on the next run.
// exercises: do-while, narrowing, linked-list
// questions: compiler.md §203.1 rule 4
// tsc: accepts; js-comparable: yes
class Node {
  v: i32;
  next: Node | null = null;
  constructor(v: i32) {
    this.v = v;
  }
}

let made: i32 = 0;

function maybe(): Node | null {
  made += 1;
  return made < 3 ? new Node(made * 10) : null;
}

export function main(): void {
  const head = new Node(1);
  head.next = new Node(2);
  head.next.next = new Node(3);
  let n: Node | null = head;
  let s: i32 = 0;
  do {
    s += n.v;
  } while ((n = n.next) !== null);
  print(`${s}`);

  let it: Node | null = maybe();
  if (it === null) {
    return;
  }
  let seen = "";
  do {
    seen += `${it.v} `;
  } while ((it = maybe()) !== null);
  print(`${seen}${made}`);
}
