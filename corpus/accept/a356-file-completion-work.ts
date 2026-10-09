// corpus: accept/a356-file-completion-work
// interpreter: no — calls the host file provider
// purpose: Run work when a host file read completes.
// exercises: file-module, host-completion, Promise.then, Promise.catch, capture, class, task-group, promise-all
// questions: compiler.md §186, §185, §181, §170, §166, collisions.md C25
// tsc: accepts
// js-comparable: no C25: Host file errors and byte arrays differ from node.
// enable-module: node:fs/promises
// file-provider: scratch

import { readFile, writeFile } from "node:fs/promises";

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

  // 1. then runs a named function when the read completes.
  const first = readFile("a.txt", "utf8").then(show);
  print("after then call");
  await first;

  // 2. A synchronous then callback captures a const object and updates its fields.
  const totals = new Totals();
  await writeFile("pending.txt", "late");
  const handles: Promise<void>[] = [];
  handles.push(readFile("pending.txt", "utf8").then((text: string): void => {
    totals.count += 1;
    totals.last = text;
    print(`then ${text}`);
  }));
  print(`before release count=${totals.count}`);
  // The test provider completes pending.txt in the next write callback.
  await writeFile("release.txt", "release");
  print("after release write");
  await Promise.all(handles);
  print(`totals ${totals.count} ${totals.last}`);

  // 3. catch handles a failed read; its callback returns a string.
  const missing = readFile("missing.txt", "utf8").catch((e: Error): string => `caught ${e.message}`);
  print("after catch call");
  print(await missing);

  // 4. A class method updates this; a TaskGroup and Promise.all wait.
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

// pin: 5875a70c
// pin-dev-jit: Exit 1; 3 errors, the first S013 "Promise combinator `.then(...)` is not in the language" at 40:43.
// pin-c-aot: Exit 1; the same 3 errors before C emission.
