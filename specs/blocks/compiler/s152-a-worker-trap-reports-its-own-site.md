<!-- §152 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 152. A worker trap reports its own site

*(Added 2026-10-03.)* Origin: an open item of `specs/tracking/s115-exceptions.md`;
owner decision of 2026-10-03 to take it before §137.3.

Problem: when a Worker traps, the parent's `join` traps with
`worker-trapped`. That trap has position 0 (no script site), and its
message embeds the Worker's raw position id, which each tier numbers
differently. Measured at `8db62b3c` with this program (the Worker's
read is at `w.ts:9:14`):

```ts
class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function crash(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) {
    const xs: i32[] = [1];
    print(`${xs[m.value]}`);
  }
}
export function main(): void {
  const w: Worker<Msg, Msg> = Worker.spawn(crash);
  w.post(new Msg(5));
  w.close();
  w.join();
  print("after");
}
```


| Tier | Output |
|---|---|
| dev | `subscript: :0:0: trap [worker-trapped]: worker trapped with index-out-of-bounds at position 6: index 5 out of bounds for array length 1` |
| ship | `trap 22 0 worker trapped with index-out-of-bounds at position 3: index 5 out of bounds for array length 1` |

The message text differs between the tiers, and neither form names the
source site.

### 152.1 Rules

1. The parent's `worker-trapped` trap carries the position of the
   Worker's trap site. The Worker runs the same program, so its position
   id resolves through the same tier's position table as any trap, and
   each tier reports the same source site.
2. The message names the Worker's trap kind and the Worker's trap
   message, and no position id: `worker trapped with <kind>: <message>`.
3. A Worker that ends without a runtime outcome keeps position 0 and
   its message.
4. The trap is raised where the parent observes the outcome (`join`),
   as before; only its position and message change.

### 152.2 Acceptance

1. Red first: a trap corpus entry with the Problem program, whose
   expected trap kind is `worker-trapped` at the Worker's read site
   (`9:14`), on the dev tier and the ship tier. At the contract pin the
   position is `0:0` and the messages differ (record both).
2. The existing tests that pin the old message text change to rule 2's
   text; each is listed with its old and new expectation.
3. No `.expected` golden moves.
