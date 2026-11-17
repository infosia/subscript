// corpus: accept/a473-foreign-call-keeps-borrowed-storage
// interpreter: no — calls the synthetic native interop library
// purpose: A foreign call keeps array storage across collection, growth, and member replacement.
// exercises: foreign-call, callback, array-storage, string-storage, member-pair, zero-copy, array-copy, explicit-collection
// questions: compiler.md §323, Q4, Q7, Q13, Q15
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.

function collectAndAllocate(): void {
  Context.collect();
  const trash: i32[][] = [];
  for (let index: i32 = 0; index < 8; index++) {
    const replacement: i32[] = [9, 9, 9, 9];
    trash.push(replacement);
  }
}

function readArray(): i32 {
  const values: i32[] = [];
  values.push(1);
  const info = new SubCallbackInfo((message, first, second) => {
    collectAndAllocate();
  }, null, null);
  return subBorrowArray(values, info);
}

export function main(): void {
  print(`${readArray()}`);
  print(`${readString()}`);
  print(`${readMember()}`);
  print(`${readPointer()}`);
  writeWithinCapacity();
  print(`${replaceMember()}`);
  print(`${replaceStringMember()}`);
  growCopiedValues();
}

function readString(): i32 {
  const value: string = "AA".slice(0, 1);
  const info = new SubCallbackInfo((message, first, second) => {
    collectAndAllocate();
  }, null, null);
  return subBorrowString(value, info);
}

function readMember(): i32 {
  const value = new SubDrawList(0, [1]);
  const info = new SubCallbackInfo((message, first, second) => {
    collectAndAllocate();
  }, null, null);
  return subBorrowMember(value, info);
}

class BorrowedPointerOwner {
  tag: i32;
  value: SubByValueI32One;
  constructor() { this.tag = 0; this.value = new SubByValueI32One(1); }
}

function readPointer(): i32 {
  const owner = new BorrowedPointerOwner();
  const info = new SubCallbackInfo((message, first, second) => {
    collectAndAllocate();
  }, null, null);
  return subBorrowPointer(owner.value, info);
}

class BorrowedValues {
  values: i32[];
  constructor(values: i32[]) { this.values = values; }
}

function writeWithinCapacity(): void {
  const state = new BorrowedValues([1, 2, 3, 4]);
  state.values.pop();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as BorrowedValues;
      Context.collect();
      state.values[0] = 7;
      state.values.push(5);
      state.values.pop();
    }
  }, state, null);
  print(`${subBorrowArray(state.values, info)}`);
  // compiler.md §323 rule 2: the borrow ends with this call.
  state.values.push(6);
  state.values.push(8);
  print(`${state.values.length}`);
}

class BorrowedMember {
  value: SubDrawList;
  constructor() { this.value = new SubDrawList(0, [1]); }
}

function replaceMember(): i32 {
  const state = new BorrowedMember();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as BorrowedMember;
      state.value.draws = [];
      collectAndAllocate();
    }
  }, state, null);
  return subBorrowMemberPointer(state.value, info);
}

class BorrowedStringMember {
  value: SubDescReadStr;
  constructor() { this.value = new SubDescReadStr("AA".slice(0, 1), 0); }
}

function replaceStringMember(): i32 {
  const state = new BorrowedStringMember();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as BorrowedStringMember;
      state.value.label = "B";
      Context.collect();
      const texts: string[] = [];
      for (let index: i32 = 0; index < 8; index++) {
        texts.push("ZZ".slice(0, 1));
      }
    }
  }, state, null);
  return subBorrowMemberString(state.value, info);
}

class CopiedValues {
  values: SubDrawList[];
  constructor() {
    this.values = [new SubDrawList(1, [1]), new SubDrawList(2, [2]),
      new SubDrawList(3, [3]), new SubDrawList(4, [4])];
  }
}

function growCopiedValues(): void {
  const state = new CopiedValues();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as CopiedValues;
      state.values.push(new SubDrawList(5, [5]));
    }
  }, state, null);
  print(`${subBorrowScratchArray(new SubBorrowDrawLists(0, state.values), info)}`);
  print(`${state.values.length}`);
}
