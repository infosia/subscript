<!-- §91 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 91. The tutorials' programs run in the gate

*(Owner decision 2026-09-08.)* Origin: the docs refresh of
2026-09-08. `docs/` last changed 2026-07-31; §66 to §90 landed after
that date, and nothing reported the drift. The refresh found four
false claims that a reader acts on, among them "there are no workers"
and "exports take no arguments". No test reads `docs/`.

### 91.1 Rule

1. **Scope.** Every fenced ` ```ts ` block in `README.md` and in
   `docs/*.md`. No block is exempt, and no marker excuses one.
2. **A program** is a block that declares `export function main` or
   `export async function main`. The gate checks it and runs it on the
   dev tier. If a ` ```text ` block follows it **immediately**, the
   program's stdout equals that block, byte for byte. *Immediately*
   means the `text` fence opens on the line after the program's
   closing fence, or one blank line after it. A `text` block that
   prose separates from the program is not the program's output, and
   the gate does not compare it. *(Defined 2026-09-08, measured: the
   first gate paired a worker program in `tutorial-rust.md` with a
   host's reload message eight paragraphs below it.)*
3. **A fragment** is any other `ts` block. The gate checks it. A
   fragment reports no diagnostic.
4. **Ambient names.** A fragment that declares `interface` or
   `declare` is given to the checker as an ambient source. A block
   that names a type the prelude does not carry is checked again with
   each committed mirror under `examples/` and `corpus/interop/`, one
   at a time; one mirror must make it clean, and the failure message
   names the mirrors tried.
4a. **An excerpt is verified as an excerpt.** A block whose first line
   is `// excerpt of <repository-relative path>` shows part of a
   tracked file. The gate requires every non-blank line of the block
   **after the first** to appear in that file, read from the working
   tree, and it checks nothing else about the block. The citation line
   is the marker and is not part of the excerpt. *(Corrected
   2026-09-08: the first text asked for every line, and the citation
   line is in no file; and it said "committed", which one round read as
   "the text at `HEAD`" — a gate that stays green at the moment a
   regenerated file drifts, and fails when a document and its file are
   fixed together.)*
   *(Added 2026-09-08, measured: rule 3 asked a generated-mirror
   excerpt to stand alone as a mirror. That needs the string-view and
   descriptor provenance records and two more declarations — a page of
   generated text where three declarations teach the shape. An excerpt
   that the gate holds line-for-line against its source catches the
   drift rule 3 exists to catch, and it stays readable. The reader
   also gets the path to the whole file.)*
5. **Siblings.** A block that imports `./name` takes the ` ```ts `
   block immediately before it as the module `name.ts`.
6. **The failure names the block.** A failure reports the file, the
   line of the block's opening fence, the first diagnostic or the
   output difference, and both byte strings when an output differs.
7. **A CLI transcript is an output too.** A ` ```sh ` block that
   immediately follows a program (rule 2's definition of immediately)
   and whose first line is `$ subscript run <file>` states that
   program's stdout in the lines below the command. The gate compares
   them. *(Added 2026-09-08: the first rule left the two "hello"
   transcripts unchecked, which are the first output a reader copies.)*
8. **The scope is asserted, not assumed.** The gate asserts the count
   of ` ```ts ` blocks it found in each document against a
   hand-written table. A fence that changes spelling, or a pairing
   that breaks, then fails the gate instead of leaving it silently.

### 91.2 Sites

- `codegen/tests/docs.rs` (new): the gate.
- `docs/tutorial-c-cpp.md`: the snippet that names `EventLog` without
  declaring it (rule 3 makes it a defect of the block).

### 91.3 Corpus and gate (pre-registered exit criteria)

1. Red at `1b44a64`, measured: the gate fails on four blocks, not one.
   `tutorial-c-cpp.md` line 813 names `EventLog` and declares it
   nowhere (S016). Line 993 names `Job` and `Total` and declares them
   nowhere (S016). Line 756 is a generated-mirror excerpt and becomes
   one under rule 4a. `tutorial-rust.md` line 451 is the pairing defect of rule 2.
   Each doc block gains the smallest declaration that makes it check;
   the pairing defect is a gate defect and the documents do not move
   for it. *(The first text of this item predicted one failure. Three
   documents carried a block a reader cannot compile, which is the
   defect class this section exists to find.)*
2. Green: every block of the four documents passes. The test prints
   the count of programs, of compared outputs, and of fragments.
3. A negative control, in the same test: a program block whose stated
   output is changed by one byte fails, and a fragment with an unknown
   name fails. The control builds the altered block in memory; it does
   not edit a document.
4. The gate runs in both profiles under `tools/gate.sh`, and it needs
   no host C toolchain, so it stays inside the headless rule
   (CLAUDE.md principle 4).
