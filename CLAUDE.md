# CLAUDE.md — subscript permanent development rules

This file holds only what is *invariant* — roles, boundaries, and
conventions. `specs/subscript-project-plan.md` holds design and phasing.
When the plan and this file disagree, this file wins; when evidence
disagrees with either, fix both.

## What this project is

**subscript** is a statically-typed, AOT-compilable **embedded** scripting
language for native host applications: a C-compatible execution and memory
model wearing a TypeScript-subset syntax. The host owns its main loop and
exposes a C ABI; subscript supplies the user-authored logic. Game engines
are the archetype and the origin of the design, not its boundary — the
same shape fits real-time audio/DSP, creative and graphics tools,
simulation, and embedded control. It is a language project, not a JS
runtime and not a JS binding.

## Roles (read first)

Implementation is done by a **separate coding agent**. **Claude plans and
orchestrates** — it authors `specs/`, emits task handoffs, reviews the
coding agent's diffs against acceptance criteria, runs builds and tests,
and manages git (`init`/`add`/`commit`). Claude does not write production
code; the coding agent does not plan, edit `specs/`, change scope, or
commit.

## Design invariants (read second)

1. **Data layout is C-ABI-identical — C, not C++.** Every language-visible
   struct lowers to the layout the platform C ABI gives the equivalent C
   struct. This is machine-verifiable (`offsetof` assertions against
   bindgen-style output) and compiler-portable, which C++ layout identity
   is not. No vtables, no C++ inheritance layout, no name mangling — ever.
2. **No implicit GC.** No collector runs unbidden: Context-scoped memory,
   manual `delete`, and collection only as an explicitly invoked host
   operation. A design that is correct only if an uninvoked collector runs
   is out of scope; a program that never collects is correct, merely
   larger.
3. **Two execution forms are both mandatory: a fast-iteration development
   tier (hot reload; realized as in-process JIT on desktop dev platforms)
   and AOT (ship).** Dropping the development tier forfeits the main
   iteration-speed argument for the language; a proposal that ships
   AOT-only is incomplete, not lean.
4. **Host interop crosses a C ABI only.** The host presents C headers; the
   language binds those. No specific host header is privileged by the
   language. If host data must become script-visible, the host grows a C
   facade — the language never binds C++ directly.
5. **The syntax is a valid-TypeScript subset.** Every accepted program must
   type-check under stock `tsc` with this project's ambient `.d.ts`
   prelude; this project's compiler then narrows semantics (soundness,
   integer types, value types). Editor tooling (tsserver) comes for free
   and is a primary reason TS syntax was chosen — a syntax extension that
   breaks `tsc` acceptance defeats it. (AssemblyScript's proven approach.)
6. **Scripts are trusted.** First-party application logic, not a sandbox.
   Spend no effort on adversarial hardening; spend it on clear, early
   errors for honest mistakes.

## Non-goals (permanent unless the plan is revised with evidence)

- **npm compatibility / running existing TypeScript code.** Sound typing
  rejects the unsound patterns the TS ecosystem is written against. This is
  a structural consequence, accepted up front — not a gap to close later.
- **JS semantics.** No `any`, no prototype mutation, no `eval`. The subset
  is defined by the collision table (`specs/blocks/collisions.md`), not by
  JS's spec.
- **Upstreaming to external projects.** *(Owner, 2026-07-27 — a
  principle, not a scheduling decision.)* When this project must change
  a dependency, it forks and pins the fork. It does not open pull
  requests, negotiate APIs, or carry patches toward acceptance
  upstream. Acceptance and timing would be outside this project's
  control, and a patch shaped for upstream's other users is a different
  and larger patch than the one this project needs. A fork is expected
  to persist; "upstream it later" is not a plan this project makes.
  Forks are still cited by URL and pinned by commit — the rule against
  filesystem paths is unaffected.
- **Being a standalone program runtime.** subscript is embedded by
  construction: the host owns the main loop and calls exported functions,
  and platform capabilities (files, sockets, devices) are the
  host's to expose through its C ABI, not the language's to provide.
  *(Threads were removed from this list — Owner, 2026-08-02: the
  standard library provides Workers (Q35), runtime-owned threads with
  per-Context isolation and copy-only messaging; the host still owns
  its main loop.)* The
  standard library grows in computation (`specs/blocks/stdlib.md`); reach
  into the outside world does not. This is a division of responsibility,
  not a capability ceiling — and not a statement about how broad the
  language's own surface may become.

## Compiler and oracle

subscript builds its own compiler and runtime. **Dev tier:** Cranelift JIT
with hot reload. **Ship tier:** C emission handed to the platform C
compiler — adopted at P4 on measured evidence (Cranelift AOT was 23× a
hand-written C baseline; emitted C is 1.05×), superseding the original
Cranelift-AOT ship tier, which is retained only as a cross-check
(`specs/blocks/compiler.md` §11; plan §8 Rev 2). The two tiers are
separate lowerings, so their agreement is established **by verification** —
the standing gate is dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact, on every
corpus entry. The oracle is the committed golden corpus outputs plus the
`tsc`-clean gate. No external implementation serves as oracle, baseline,
or cross-check.

## Language

- **All repository documentation, specs, comments, and identifiers:
  English.**
- Conversation with the user (chat responses): Japanese.

## Writing style (applies to everything: specs, tracking, comments, commit
## messages, and chat reports)

**State fact, evidence, and consequence. Nothing else.**

Forbidden: rhetorical or dramatic framing; narrating a mistake as a story;
restating a lesson in more than one document; self-referential commentary;
emphatic repetition.

Required: a lesson worth keeping is written **once**, as a one-line rule.
Reports state what changed, what was measured, what is next. A correction
states what was wrong, the evidence, and the corrected claim.

**Rule: a claim about another system's behaviour requires running that
system.** Claims taken from documentation alone are marked *(docs)* where
they appear.

### Simplified Technical English

*(Owner, 2026-08-04.)* Write all English here in ASD-STE100 Simplified
Technical English (<https://www.asd-ste100.org/>), pragmatic mode: apply
the structural rules and keep the domain vocabulary. The rule set this
project follows is the `simple-english` agent skill
(<https://github.com/AminBlg/SimpleEnglish>).

The rules that matter most here:

- Maximum 20 words in an instruction. Maximum 25 in a description.
- One word, one meaning, for the whole document. Select one verb for the
  check/verify/confirm concept, then use no other word for it.
- Simple tenses only. Do not write "has been" or "is being".
- Do not use an `-ing` form as a verb.
- Use the active voice.
- `can`, `will`, and `must` are approved. `should`, `would`, `may`,
  `might`, and `could` are not. A requirement is `must`. Write a
  suggestion as a fact, or delete it.
- Put the condition before the command: "If the build fails, read the
  log."
- Write one instruction per sentence.
- Keep the articles and keep "that". STE is short, not terse.
- For a risk, give the command first and the consequence second.

Never rewrite code, identifiers, commands, or quoted error text.

This standard is for English. Japanese chat replies follow the same
intent — short sentences, one term for one concept, no hedging — but the
numbered rules do not apply to them.

## Core principles

1. **Every public API has a direct unit test**, shipped in the same commit.
2. **Corpus-first.** The target-program corpus and the rejected-program
   corpus (`corpus/accept/`, `corpus/reject/`) are the language's
   executable definition. A syntax or semantics decision without a corpus
   entry is not decided. A sound language is defined as much by what it
   rejects as by what it accepts.
3. **Differential testing.** From the moment the second execution form
   exists (plan P3), the same corpus program runs under both execution
   forms (the fast-iteration development tier and the AOT tier —
   invariant 3) with byte-identical output, checked against the committed
   golden outputs, on every test run. Before that point goldens are
   provisional (`specs/blocks/compiler.md` §2).
4. **Headless-first.** Every gate passes with no GPU, no window, and no
   external device. Device-dependent runs are gated, never required for
   CI.
5. **No panics in library code**; `Result` and `?`. The FFI boundary is the
   single exception and never unwinds across a C boundary.
6. **Generated code is never hand-edited.** Fix the generator.
7. **Exit criteria before implementation.** Every phase's spec names, in
   advance, the measurement that would kill or pass it.

## Code conventions

`#[non_exhaustive]` on extensible public enums/structs; `#[must_use]` on
builders; `///` docs on every public item with `#![warn(missing_docs)]`;
every `unsafe impl Send`/`Sync` carries a `// SAFETY:` comment; `#[allow]`
on a correctness or soundness lint requires a justifying comment (blanket
allows only on generated-bindings modules); C↔Rust conversions macro-driven
in one module; colocate each area's code with its own module.

**Formatting** *(Owner, 2026-08-09)*: the tree is rustfmt-canonical
under the toolchain `rust-toolchain.toml` pins. `cargo fmt --check`
is a standing gate. Do not run a formatter from any other toolchain
version.

## Workflow per area

1. Write/extend `specs/blocks/<area>.md` — contract first.
2. Add corpus entries (accept + reject) — Red.
3. Implement — Green.
4. Run the tier-differential suite.
5. Log evidence in `specs/tracking/<topic>.md`.

**Every phase ends with a mandatory Phase Review ("Clean Review Then Fix"):**
a fresh no-context subagent reviews the phase's cumulative diff and emits
`CRITICAL`/`MAJOR`/`MINOR` findings; findings are fixed in severity order; a
phase cannot be COMPLETE with any open CRITICAL/MAJOR.

## Privacy / repo hygiene

- No credentials, signing material, or device-specific secrets committed.
- `.gitignore`: `target/`, `node_modules/`, `.claude/`, local test
  transcripts.

### No local or sibling paths in committed files

**Nothing committed to this repository may reference a path outside it.**
Applies to every tracked file — docs, specs, comments, code, tests,
`build.rs`, CI config, and commit messages.

Forbidden: absolute paths into any developer's filesystem; relative paths
escaping the repository root (including sibling checkouts); machine- or
user-specific names; references to predecessor or sibling projects by name
or path.

Required instead: cite external projects by upstream URL and by their own
repo-relative paths; pin external sources as git submodules or fetched
artifacts resolved by `build.rs` / env var with a documented default. When
a claim was verified against a local checkout, record the finding and the
upstream citation, never the local path used to reach it.

## Tooling — sandbox

Avoid `dangerouslyDisableSandbox: true` whenever possible. Network ops
(`git push`/`pull`, `npm install`, submodule fetches) are invoked by the
user via the `!` prompt, not run by Claude with the sandbox disabled.
