// corpus: accept/a475-c-replaced-members
// interpreter: no — calls the synthetic native interop library
// purpose: Receives C array replacements and nullable pointer writes.
// exercises: foreign-call, pair-read-back, pointer-compare, string-view
// questions: Q13
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.

export function main(): void {
  const list = new Sub325List(0, [1, 2]);
  sub325Replace(list);
  print(`${list.items.length} ${list.items[0]} ${list.items[1]} ${list.items[2]}`);
  const empty: i32[] = [];
  const fromEmpty = new Sub325List(0, empty);
  sub325Replace(fromEmpty);
  print(`${fromEmpty.items.length} ${fromEmpty.items[2]}`);
  const indirect = new Sub325Holder(new Sub325List(0, [1, 2]));
  sub325Indirect(indirect);
  if (indirect.list !== null) print(`${indirect.list.items.length} ${indirect.list.items[1]}`);
  const first = new Sub325First(0, [1, 2]);
  sub325First(first);
  print(`${first.items.length} ${first.items[0]}`);
  const texts: Sub325Text[] = [];
  const textList = new Sub325Texts(0, texts);
  sub325Texts(textList);
  Context.collect();
  print(`${textList.items[0].label} ${textList.items[1].label} ${textList.items[0].value} ${textList.items[1].value}`);
  const original: i32[] = [1, 2, 3];
  const written = new Sub325List(0, original);
  sub325Write(written);
  print(`${written.items[0]} ${written.items[1]} ${written.items[2]}`);
  original[0] = 91;
  print(`${written.items[0]}`);
  sub325Empty(list);
  print(`${list.items.length}`);
  const pointer = new Sub325Scratch([1], new Sub325Leaf(1));
  sub325Null(pointer);
  print(`${pointer.leaf === null}`);
  const links: Sub325Link[] = [];
  const received = new Sub325Links(0, links);
  sub325ReceivedNull(received);
  print(`${received.items.length} ${received.items[0].leaf === null}`);
  const descriptor = new Sub325Descriptor([1]);
  sub325Descriptor(descriptor);
  print(`${descriptor.items.length} ${descriptor.items[2]}`);
  const scratchItems: Sub325TextLink[] = [];
  const scratchReceived = new Sub325TextLinks(0, scratchItems);
  sub325ReceivedScratchNull(scratchReceived);
  Context.collect();
  print(`${scratchReceived.items[0].label} ${scratchReceived.items[0].leaf === null}`);
  const groups: Sub325MutableLinks[] = [];
  const nested = new Sub325Nested(groups);
  sub325NestedReceive(nested);
  Context.collect();
  print(`${nested.groups[0].items.length} ${nested.groups[0].items[0].tag} ${nested.groups[0].items[1].leaf === null}`);
  const firstLeaf = new Sub325Leaf(71);
  const secondLeaf = new Sub325Leaf(82);
  const capacity: Sub325Link[] = [new Sub325Link(1, firstLeaf), new Sub325Link(2, secondLeaf)];
  const shorter = new Sub325Links(0, capacity);
  sub325Shrink(shorter);
  Context.collect();
  const keptLeaf = shorter.items[0].leaf;
  if (keptLeaf !== null) print(`${shorter.items.length} ${keptLeaf.value} ${capacity.length}`);
  const writable = new Sub325MutableLinks(capacity);
  sub325ShrinkMutable(writable, 0);
  const writableLeaf = writable.items[0].leaf;
  if (writableLeaf !== null) print(`${writable.items.length} ${writableLeaf.value}`);
  const nullCapacity: Sub325Link[] = [new Sub325Link(1, firstLeaf), new Sub325Link(2, secondLeaf)];
  const nullShorter = new Sub325MutableLinks(nullCapacity);
  sub325ShrinkMutable(nullShorter, 3);
  print(`${nullShorter.items.length} ${nullShorter.items[0].leaf === null}`);
  const scratchCapacity: Sub325TextLink[] = [new Sub325TextLink("one", firstLeaf), new Sub325TextLink("two", secondLeaf)];
  const shorterScratch = new Sub325TextLinks(0, scratchCapacity);
  sub325ShrinkScratch(shorterScratch);
  Context.collect();
  const scratchLeaf = shorterScratch.items[0].leaf;
  if (scratchLeaf !== null) print(`${shorterScratch.items.length} ${shorterScratch.items[0].label} ${scratchLeaf.value}`);
  const callbacks: SubCallbackInfo[] = [new SubCallbackInfo((message, userdata, userparam) => { print(message); }, null, null), new SubCallbackInfo((message, userdata, userparam) => {}, null, null)];
  const shorterCallbacks = new Sub325Callbacks(0, callbacks);
  sub325ShrinkCallbacks(shorterCallbacks);
  Context.collect();
  print(`${shorterCallbacks.items.length} ${shorterCallbacks.items[0].userdata === null}`);
  shorterCallbacks.items[0].callback("callback kept", null, null);
  const watchGroups: Sub325WatchGroup[] = [new Sub325WatchGroup("kept", capacity), new Sub325WatchGroup("removed", capacity)];
  const shorterGroups = new Sub325WatchGroups(0, watchGroups);
  sub325ShrinkGroups(shorterGroups);
  Context.collect();
  const groupLeaf = shorterGroups.groups[0].items[0].leaf;
  if (groupLeaf !== null) print(`${shorterGroups.groups.length} ${shorterGroups.groups[0].label} ${groupLeaf.value}`);
}
