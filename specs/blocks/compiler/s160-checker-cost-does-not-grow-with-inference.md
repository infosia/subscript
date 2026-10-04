<!-- §160 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 160. Checker cost does not grow with inference

*(Added 2026-10-05.)* Origin: the owner asked whether §156 made the
checker slower than its rules need, or spends time on contrived forms
(2026-10-05). A cost defect, not a surface change.

Problem: §156 made `subscript check` slower on programs that do not use
inference. Measured with release builds of `40b0cbff` (before §156) and
`5e708e40`, best of three runs, on a generated program of `n` blocks.
Each block holds a module constant, a class with four initialized
fields and two methods (a defaulted parameter, a loop, a branch, a
`map` callback), and a function that calls it. The annotated and the
inferred forms are the same program with and without the annotations
that §156 makes optional.

| `n` | `40b0cbff`, annotated | `5e708e40`, annotated | `5e708e40`, inferred |
|---:|---:|---:|---:|
| 200 | 0.07 s | 0.22 s | 0.22 s |
| 1,000 | 0.34 s | 3.84 s | 3.84 s |
| 3,000 | 2.25 s | 35.46 s | 63.48 s |

The 293 single-file accept entries take 1.37–1.40 s before and 1.41–1.44
s after (process start dominates). A program written without inference
became 16 times slower at `n` = 3,000, so the cost is not the cost of
inference.

### 160.1 Rules

1. A program that uses no §156 inference costs no more to check than at
   `40b0cbff`, within the noise of the measurement.
2. The cost of a §156 decision is proportional to the part of the
   initializer that the type needs (§156 rule 3), checked once. No
   decision copies a whole function context, scans every declaration,
   or checks a body again, per declaration.
3. A check that exists only for a contrived form (CLAUDE.md
   invariant 6: a form that only a hostile or contrived program reaches) costs nothing on
   a program that does not contain the form.

### 160.2 Acceptance

1. A measurement round first: find each cost that rules 1–3 forbid, by
   timing counters or a profile, and record each with its share of the
   time on the `n` = 3,000 programs. Record which costs serve a
   contrived form.
2. After the fix, on the same machine, release builds, best of three:
   - annotated `n` = 3,000: at most 1.2 times the `40b0cbff` time;
   - inferred `n` = 3,000: at most 1.5 times the annotated time of the
     same build;
   - the scaling from `n` = 1,000 to `n` = 3,000 is no worse than at
     `40b0cbff`.
3. No diagnostic changes on any corpus entry, and no golden moves.
4. The generator of the measured programs is committed as a test
   fixture generator or a tracking-note script, so the measurement can
   be repeated.

### 160.3 Open

1. Rule 1 is not met for annotated field initializers: 6,000 classes
   with four annotated fields check in 1.16–1.17 times the `40b0cbff`
   time (0.215 s against 0.185 s for fields that read earlier fields,
   0.146 s against 0.125 s for constant fields, release, best of three,
   2026-10-05). The fix of the Phase Review removed the §156 decision
   path from annotated fields (1.5 times before it); the residual is in
   the per-field checks that §156 and §158 share. About 5 µs per class.
2. The §158 local analysis is linear for independent `if` and `switch`
   statements (2,000 uninitialized locals: 4.05 s to 0.31 s, equal to
   initialized locals). One large nested statement, or many exits that
   every local shares, still costs one traversal per local.
