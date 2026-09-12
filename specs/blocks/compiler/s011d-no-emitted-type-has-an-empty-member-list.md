<!-- §11d of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11d. No emitted type has an empty member list

*(2026-08-28.)* C11 6.7.2.1 gives a structure or a union at least one
member, and 6.7.2.2 gives an enumeration at least one enumerator. GCC and
clang accept an empty one as an extension. MSVC rejects it (`C2016`), and
an initializer on it then reports `C2078`. The emitted C must compile on
every ship and dev target, so an emitted type with no member is a defect
wherever it is produced.

**This class has two recorded instances.** The first was
`typedef struct Sub_N_EngWorld {}` for a zero-field opaque handle, closed
by giving it `char subscript_opaque;`. The second was the shadow-root
frame, whose declaration read a module fact and whose members read
function facts. Both are recorded in
`specs/tracking/windows-portability.md`.

CLAUDE.md's two-round rule applies at a second instance: a fix that
closes named sites does not converge. So the emitter carries a **total
check over its own output**, not a rule at each producer.
`emit_lir_c` scans the finished translation unit and fails with **every**
empty `struct`, `union`, or `enum` body it finds, each with its line and
its declared name. The check reads the emitted text, and the C standard
supplies the rule, so it compares two facts derived apart (core principle
9). A new producer of an empty type meets a build failure naming its
site, and no future round needs to find the site by hand.

The check is not a formatter and not a C parser. It skips comments and
string and character literals, so a brace inside either is not a member.
