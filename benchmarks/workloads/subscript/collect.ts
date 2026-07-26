// benchmark: collect
// Build six graphs of 20000 48-byte nodes from the fixed LCG. Every node
// owns four unique strings with lengths 9/41/105/233. Since a subscript
// string requests 8+len payload bytes, those are deliberately unaligned
// 17/49/113/241-byte requests: one byte past the 16/48/112/240-byte
// size-class payload capacities.
//
// Nodes with (state&3)!=0 survive (exactly 15000 per round); the rest and the
// preceding survivor graph are dropped before collect(). Checksum per node in
// each reverse-built survivor chain:
//   checksum = checksum*31 + state + 9 + 41 + 105 + 233 (i32 wrap).

const COUNT: i32 = 20000;
const ROUNDS: i32 = 6;

class Node {
  value: i32;
  s9: string;
  s41: string;
  s105: string;
  s233: string;
  next: Node | null;

  constructor(
    value: i32,
    s9: string,
    s41: string,
    s105: string,
    s233: string,
    next: Node | null,
  ) {
    this.value = value;
    this.s9 = s9;
    this.s41 = s41;
    this.s105 = s105;
    this.s233 = s233;
    this.next = next;
  }
}

export function main(): void {
  let state: i32 = 0x12345678;
  let checksum: i32 = 0;
  let keep: Node | null = null;
  let cursor: Node | null = null;
  // These construction roots are cleared before collection. C emission uses
  // one conservative shadow slot per managed local, even after block exit.
  let suffix: string = "";
  let s9: string = "";
  let s41: string = "";
  let s105: string = "";
  let s233: string = "";

  for (let round: i32 = 0; round < ROUNDS; round += 1) {
    // Dropping keep makes the preceding round's survivor graph reclaimable.
    keep = null;
    let dropped: Node | null = null;

    for (let i: i32 = 0; i < COUNT; i += 1) {
      state = state * 1664525 + 1013904223;
      const uid: i32 = round * COUNT + i;
      suffix = `${uid}`;
      s9 = suffix.padStart(9, "a");
      s41 = suffix.padStart(41, "b");
      s105 = suffix.padStart(105, "c");
      s233 = suffix.padStart(233, "d");
      if ((state & 3) !== 0) {
        keep = new Node(state, s9, s41, s105, s233, keep);
      } else {
        dropped = new Node(state, s9, s41, s105, s233, dropped);
      }
    }

    dropped = null;
    suffix = "";
    s9 = "";
    s41 = "";
    s105 = "";
    s233 = "";
    collect();

    cursor = keep;
    while (cursor !== null) {
      checksum = checksum * 31 + cursor.value;
      checksum = checksum + cursor.s9.length;
      checksum = checksum + cursor.s41.length;
      checksum = checksum + cursor.s105.length;
      checksum = checksum + cursor.s233.length;
      cursor = cursor.next;
    }
  }

  keep = null;
  collect();
  print(`${checksum}`);
}
