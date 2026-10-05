<!-- §165 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 165. The async documentation states the contract

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 1 (documentation). The owner selected the
documentation part on 2026-10-06. The editor diagnostics part is not in
this section.

Problem: the tutorials state async behaviour that the contract does not
have. A reader who follows them writes a wrong program or a wrong host
loop. Measured at `49996dca` (`subscript run`):

| Place | Text | Contract and measurement |
|---|---|---|
| `docs/tutorial-typescript.md:576` | "A failed `await` returns a value; it does not throw." | §116 rule 2: the `await` raises the exception. `try { await fails(); } catch (e) { … }` prints `caught boom`. |
| `docs/tutorial-typescript.md:409–411` | "`finally` is rejected, and so is a `try` block that holds `await` or `yield`." | §116 admits a `try` block across an `await`; a generator `try` around `yield` runs its `catch`. Both are accepted and run. |
| `docs/tutorial-c-cpp.md:187–194` | the host step resumes "every pending async entry once, in start order"; "promises are not storable values, so there is nothing to collect" | §94.1 rules 8 and 9: a checkpoint makes the parked frames runnable and drains the ready queue. §70: a handle is a Context-owned frame that a local, an array, or a field holds, released by count. |
| `docs/tutorial-c-cpp.md:1384–1386` | `subscript_rt_ctx_async_step` resumes "every pending entry once (start order, deterministic)" | §94.1, as above; `runtime/include/subscript_runtime.h` states the drain. |
| `docs/tutorial-rust.md:594` | `session.async_step()` "polls each root pending at entry once" | §94.1, as above. |

The TypeScript tutorial also omits three facts that a program meets:
each `await` of a failed handle raises the exception again (§116 rule
3); a failed handle that no `await` observes traps when its last holder
releases it (§116 rule 4, `corpus/trap/t66-unobserved-async-exception.ts`);
and the language reference (Q34) does not state exception delivery.

### 165.1 Rules

1. Each text of the table is replaced with the contracted behaviour. The
   replacement cites its section.
2. The TypeScript tutorial async part states, each with a program fence
   whose output the §91 documentation test checks:
   - a `try` around an `await` of a failed call catches the exception;
   - two `await`s of one failed handle each raise it.
3. The TypeScript tutorial states that a failed handle that no `await`
   observes traps when its last holder releases it, and cites
   `corpus/trap/t66-unobserved-async-exception.ts`. A trap is not
   program output that the §91 test checks.
4. The language reference Q34 entry, from
   `compiler/src/language_reference.rs`, states §116 rules 1 to 4 in one
   paragraph. `generated-docs/language-reference.md` is regenerated.
5. No compiler, runtime, or corpus behaviour changes.

### 165.2 Acceptance

1. A search of `README.md`, `llms.txt`, `docs/`, `generated-docs/`, and
   `runtime/include/` for the texts of the table finds none. The
   tracking note lists the search and each replaced text.
2. The §91 documentation test passes, and runs the two new fences of
   rule 2 with their output.
3. The generator output is current: regenerating it changes no file.
4. No `.expected` golden moves.

### 165.3 Texts outside the table

The Phase Review found these texts outside the table. Each is stale or
incomplete. The owner added them to this section on 2026-10-06. Rule 1
applies to each, and each replacement cites its section.

1. The doc comment of `ReloadSession::async_pending`
   (`codegen/src/reload.rs`) counts "suspended async roots" and names
   "polling control". §94.2 counts ready jobs and parked registrations.
2. The doc comment of `Context::async_kick` (`runtime/src/context.rs`)
   names one "deterministic pending queue". §94.1 rules 3 and 4 put a
   suspended root on the parked list or on the waiter list of a child.
3. No text states §116.1 rule 5. The TypeScript tutorial and the Q34
   entry state it: an exception that leaves a host-callable async
   export traps (29), because the export has no holder. Measured:
   `export async function main(): Promise<void> { throw new Error("export boom"); }`
   traps with `[uncaught-exception]: Error: export boom`, exit 1.
4. `docs/tutorial-typescript.md` (near line 510) and the Q34 entry list
   a local and an array as handle holders. §70.3 rule 2a also admits a
   field and a global. Each list names all four. The Q34 sentence does
   not use "may".
5. `docs/tutorial-c-cpp.md` states `TrapKind` "kinds 1 through 27 at
   this commit"; the kinds run to 32. The text names
   `runtime/src/trap.rs` as the list and states no count.

Acceptance: a search for each quoted stale text finds none, and the
tracking note lists each replacement. The §91 test and the
`language_reference` tests pass.
