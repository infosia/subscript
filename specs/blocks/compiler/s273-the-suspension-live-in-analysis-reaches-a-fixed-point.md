<!-- §273 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 273. The suspension live-in analysis reaches a fixed point

*(Added 2026-11-13.)* Origin: §271.4 item 3, a finding of the Phase
Review of §271. Under the owner's instruction of 2026-11-13 to continue
with the review findings.

Problem, measured at `4bbe4321` with the debug CLI and at `09d99a57` (before
§271) with a CLI built at that commit. `gen()` yields 1 and 2. `tsc`
accepts each form, and `node` prints `end 2`. The development tier and
the ship tier do not end the first form.

```ts
function* gen(): Generator<i32> { yield 1; yield 2; }
export async function main(): Promise<void> {
  let n: i32 = 0;
  for (let r: i32 = 0; r < 1; r++) {
    for (const j of [0, 1]) { if (j > 0) { break; } }
    for (const x of gen()) { await Context.suspend(); n++; }
  }
  print(`end ${n}`);
}
```

| Change to the form | Result |
|---|---|
| none | does not end |
| `continue` in place of `break` | does not end |
| a `while` loop as the outer loop | does not end |
| an empty `if (j > 0) {}` in the array loop | does not end |
| no `if` in the array loop | `end 2` |
| no outer loop | `end 2` |
| no `await` in the generator loop | `end 2` |

The compiler does not end: the program never runs. The coding agent
found the cause. `thread_suspension_live_ins`
(`codegen/src/lir/liveness.rs`) gives each block a version of each
origin that a suspension keeps live. It also gives a version to a block
where the origin is not live. Where the versions of the predecessors
differ, it takes the version with the smallest value number. Around a
loop this choice changes at each round: in the first form, the
generator handle `%30` and the merge parameter `%48` change places at
the blocks `b5` and `b7` of the array loop at each round. These blocks
come before the definition of the handle. So the loop of the analysis
always finds a change and does not end. A conditional in the array loop
adds the blocks that make the cycle.

### 273.1 Rules

1. The analysis rebuilds the SSA form of each origin that a suspension
   keeps live, by the standard construction, with no loop to a fixed
   point. The definitions of an origin are its first definition and its
   new definition in the successor of each suspension. A block on the
   iterated dominance frontier of those definitions takes a merge
   parameter for the origin if the origin is live in it by plain
   liveness. A walk of the dominator tree gives each use the version
   that reaches it.
2. The analysis does not choose one version by its value number.
3. The construction ends for each function, because it visits each block
   a bounded number of times for each origin.
4. A merge parameter of an origin that is live by plain liveness stays,
   wherever two versions reach the block. A first attempt pruned with its
   own result and dropped four live parameters of `a439`; the coding
   agent measured each of them as live by the plain liveness of the pin.
5. The three tiers give the output of `node`. Each program that ended
   before this section gives its golden output. A change of the LIR of
   such a program is one of these: a new number of a value, a merge
   parameter added or removed for an origin that is not live in that
   block by plain liveness, or a removed trivial merge, where each
   predecessor passes the same value. In `a439`, `main` block `b76`
   loses four trivial merges of live origins (`%330`, `%345`, `%360`,
   `%374`); both predecessors pass the same value to each.
6. The dominators read only the predecessors that the entry block
   reaches, so a block with no path from the entry does not change the
   dominators of a reachable block.

### 273.2 Acceptance

1. Red first, at the pin: the accept entry
   `a445-the-suspension-live-in-analysis-reaches-a-fixed-point` with the
   first form, the `continue` form, the `while` form, and the empty `if`
   form. A
   test runs it with a time limit, and at the pin it does not end. The
   tracking note records the cause of rule 1.
2. After the change, `a445` passes `tsc`, the three tiers, and `node`.
3. A unit test of `thread_suspension_live_ins` builds the LIR of the
   first form and of the form with an empty `if`, and the analysis ends.
   A control: the form with no `if` gives the same LIR as at the pin.
4. `main` of `a445` is async, so the LIR text snapshot grows by its
   block. The snapshot also changes in the blocks of the entries that
   the tracking note lists, each change of a kind that rule 5 names. No
   other golden moves.

### 273.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read the construction: the definitions, the
frontier, the work list that adds each block once, the pruning by the
liveness before the new parameters, and the rename over the dominator
tree. No loop to a fixed point remains. A handler block takes no merge
parameter, because each raise site has its own landing block. Programs
with loops, `continue`, `break`, a nested generator, an early `return`
from a loop over a generator after an `await`, nested awaits, awaits in
each arm of `try`, `catch`, and `finally`, `switch`, and `do…while` end
and print what `node` prints, in the development tier and the ship
tier. A function with 100 generator loops compiles in 4.05 seconds in
the debug CLI, and in 2.80 seconds with no `await`; the cost before
this section was not measured.

1. MINOR, realistic, fixed: rule 5 did not admit a removed trivial merge
   of a live origin, and acceptance item 4 did not name the entries
   whose snapshot moves.
2. MINOR, contrived, fixed: the dominators read a predecessor that the
   entry does not reach. Rule 6 states it.
3. MINOR, fixed: no test ran the Red form under a time limit.
4. MINOR, realistic, fixed: two tests ran corpus entries on the three
   tiers, which the corpus gate already does.
5. MINOR, fixed: the control compared a whole module of 359 lines, so
   an unrelated change moved it.
6. MINOR, fixed: the tracking note cited contract commits that the
   history no longer holds, and kept drafts that the result contradicts.
7. MINOR, contrived, recorded: §273.4 item 1.

### 273.4 Open

1. In `a341`, function `arrays`, block `b10`, the invalidation list of
   an instruction names a value that does not dominate the block, before
   and after this section. Only the LIR verifier reads the list.
