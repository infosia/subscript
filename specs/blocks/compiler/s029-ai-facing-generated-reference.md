<!-- §29 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 29. AI-facing generated reference

Owner decision 2026-08-01. Coding agents need the **current state**
of the language, not its decision history; `specs/blocks` is
history-organized by design and stays that way. The answer is
derived documents beside the §17 API reference — generated, never
hand-edited, with the same regeneration gate — plus one small
hand-written entry point.

### 29.1 `generated-docs/language-reference.md`

One generator command produces the whole `generated-docs/` set. The
language reference contains, in order: a compact current-state
surface summary (curated prose blocks embedded in the generator
source — single-sourced and code-reviewed, the §17.1 discipline);
every `RuleCode` with its `explanation()` and a **machine-excerpted
minimal rejection** — the pinned line (with two lines of context)
from the reject-harness table's (file, code, line) entry for that
code, plus the entry's header-comment guidance where present; every
`WarnCode` likewise from the warn pins; and per-feature sections
(sized numerics, value and reference classes, Q33 descriptors, Q32
literal unions, Q34 async, modules, coroutines, the memory model)
each linking its corpus entries. Codes with multiple pinned entries
pick the first; a code with no corpus pin fails the generator loud.

### 29.2 `generated-docs/corpus-index.md`

A generated table per corpus arm (accept, reject, warn, trap) from
the entries' structured header comments — name, `purpose:`,
`exercises:`, `questions:` — with the expected-output file beside
each accept/trap entry. An entry missing the header fields fails
the generator loud, which turns the existing comment convention
into an enforced one.

### 29.3 `llms.txt`

Repo root, hand-written, small and stable: the read order
(language-reference → api-reference → corpus-index), the working
loop (`subscript check` renders each error with its rule text;
warnings carry W-codes; `subscript run` executes under the dev
tier), and the instruction **not** to read `specs/` for
current-state answers (it is the decision history). Tutorials are
listed as secondary, human-paced material.

### 29.4 Exit criteria (pre-registered)

1. A gate test regenerates both generated files and byte-compares
   them to the committed copies (the §17.4 pattern).
2. Every `RuleCode` and `WarnCode` appears with a corpus-backed
   excerpt; the generator fails loud on a code without a pin or an
   entry without headers.
3. The index covers every entry of all four corpus arms.
4. Full gate and `tsc` gate green; no golden moves; the generator
   runs offline.
5. `llms.txt` names the read order and the check/run loop.
