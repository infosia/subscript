# A nullable string — `compiler.md` §218

Status: contract.

The owner selected this section and §219 on 2026-10-31. The Step 0 table
is in `specs/tracking/nullable-array.md`.

A prototype of 47 lines in 12 files at `14a143f1` closed the wrong values
of the table: 70 programs equal `node`, and 104 programs rejected or
trapped the same way on each tier. It was reverted, and it ran no gate.
It did not sweep the `Type::Str` sites.

The interpreter has two null forms, `Value::Null` and
`Value::Handle(null)`.
