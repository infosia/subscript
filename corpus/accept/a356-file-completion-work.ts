// corpus: accept/a356-file-completion-work
// interpreter: no — calls the host file provider
// purpose: Run work when a host file read completes.
// exercises: file-module, host-completion, async-function, async-arrow, capture, class, task-group, promise-all
// questions: compiler.md §185, §181, §170, §166, collisions.md C25
// tsc: accepts
// js-comparable: no C25: Host file errors and byte arrays differ from node.
// enable-module: node:fs/promises
// file-provider: scratch

import { readFile, writeFile } from "node:fs/promises";

async function readThen(path: string, cb: (text: string) => void): Promise<void> {
  const text = await readFile(path, "utf8");
  cb(text);
}

async function readThenAsync(path: string, cb: (text: string) => Promise<void>): Promise<void> {
  const text = await readFile(path, "utf8");
  await cb(text);
}

function show(text: string): void {
  print(`show ${text}`);
}

class Totals {
  count: i32 = 0;
  last: string = "";
}

class Loader {
  name: string;
  count: i32 = 0;
  text: string = "";
  constructor(name: string) { this.name = name; }
  async load(path: string): Promise<void> {
    const text = await readFile(path, "utf8");
    this.count += 1;
    this.text = `${this.text}${text}`;
    print(`${this.name} loaded ${text}`);
  }
}

export async function main(): Promise<void> {
  await writeFile("a.txt", "alpha");
  await writeFile("b.txt", "beta");

  // 1. A named function runs when the read completes.
  const first = readThen("a.txt", show);
  print("after readThen call");
  await first;

  // 2. An async arrow captures a const object and updates a field.
  const totals = new Totals();
  await writeFile("pending.txt", "late");
  const handles: Promise<void>[] = [];
  handles.push(readThenAsync("pending.txt", async (text: string): Promise<void> => {
    totals.count += 1;
    totals.last = text;
    print(`arrow ${text}`);
  }));
  print(`before release count=${totals.count}`);
  // The test provider completes pending.txt in the next write callback.
  await writeFile("release.txt", "release");
  print("after release write");
  await Promise.all(handles);
  print(`totals ${totals.count} ${totals.last}`);

  // 3. A class method updates this; a TaskGroup and Promise.all wait.
  const loader = new Loader("group");
  const group = new TaskGroup();
  group.add(loader.load("a.txt"));
  group.add(loader.load("b.txt"));
  print("group started");
  await group.join();
  print(`group ${loader.count} ${loader.text}`);

  const other = new Loader("all");
  const jobs: Promise<void>[] = [other.load("b.txt"), other.load("a.txt")];
  print("all started");
  await Promise.all(jobs);
  print(`all ${other.count} ${other.text}`);
}

// pin: e3bd0bbc
// pin-dev-jit: Exit 0; output matches the golden.
// pin-c-aot: Exit 0; output matches the golden.
