// corpus: accept/a291-host-api-aliases/audio
// purpose: Keeps the library update local to the module API.
// exercises: entry-module, named-export
// questions: compiler.md §129, C18
// tsc: accepts; js-comparable: yes
export function update(): void { print("audio 2"); }
