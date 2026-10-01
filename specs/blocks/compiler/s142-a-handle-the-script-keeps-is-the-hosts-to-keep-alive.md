<!-- §142 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 142. A handle the script keeps is the host's to keep alive

*(Added 2026-10-01.)* Origin: a design review of the host API
(`HANDOFF.md`, 2026-10-01, item 2); owner decision of the same day to
take this item and defer the others.

Problem: §59 rule 5 says that "a parameter is a borrow for the duration
of the call" and, in the next sentence, that "the script can wrap and
store" a handle. The two sentences do not say what the host owes after
the call returns. `a137` stores a host handle in a script object
(`adopted = new AdoptedState(state)`) and uses it in a later entry
call, and the language accepts and runs it. No contract, header
comment, or example states the host's obligation for a stored handle.
The same gap holds for a handle that a foreign call returns. The
runtime's freed-handle diagnostics cover runtime allocations only; they
do not see the destruction of a host object.

### 142.1 Rules

1. A handle is a copyable value that names a host object. A handle
   that the host gives to a script, as an entry parameter or as a
   foreign-call result, transfers no ownership. The script can copy it and keep it in any
   object, closure, or module global, and use it in a later call.
2. The host keeps the named object valid for as long as any script
   code of that Context can use the handle. A script object that holds
   the handle keeps nothing alive on the host side. The language and
   the runtime do not detect a use of a handle after the host destroys
   its object.
3. The baseline the documentation gives is a Context-scoped lifetime:
   the host keeps an object that it passed to a script valid until the
   Context and all its script activity (pending async work included)
   have ended. A host that destroys objects earlier owns the protocol
   that makes it safe; `examples.md` lists the patterns (an explicit
   detach point, reference counting with explicit retain and release,
   an ID with a generation counter that the host validates at each
   access).
4. §59 rule 5 reads: "At the C level, a parameter is passed by value
   for the duration of the call. A handle parameter follows §142." The
   borrow wording retires.
5. The host-facing text says the same: the generated runtime header,
   at the host-entry calling convention, and the generated program
   header, at the entry declarations, state rules 1 and 2 in one
   short comment each, once per header, inside the documentation block
   of the declaration it describes. The program header holds a copy of
   the runtime header text, so the comment there appears once, from
   that copy. Rule 3's patterns live in `examples.md`, not in the
   headers. The tutorials (`docs/tutorial-rust.md`,
   `docs/tutorial-c-cpp.md`) state rules 1 and 2 where they describe
   handle parameters.

### 142.2 Acceptance

1. No compiler or runtime behavior changes; no corpus entry is added.
   `a137`'s header names §142 in its `questions` line.
2. The generated headers carry the comments of rule 5; the generator
   tests that pin the header text are updated with the full new text,
   and the byte-identical regeneration of every committed header
   passes after regeneration through the generator.
3. `examples.md` has a short subsection with the rule 3 patterns.
4. No `.expected` golden moves.
