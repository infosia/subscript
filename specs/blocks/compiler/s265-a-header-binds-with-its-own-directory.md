<!-- §265 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 265. A header binds with its own directory

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R06.
Selected by the owner on 2026-11-13.

Problem, read at `1ec23a63`. `subscript bind` reads the header file and
passes its text to the bindgen frontend (`cli/src/lib.rs`
`bind_command`). The frontend gives libclang the text as an unsaved
file named `header.h` in the current directory
(`bindgen/src/clangfe.rs` `parse_inner`). A quoted include such as
`#include "types.h"` resolves against the directory of the including
file, so libclang looks for `types.h` in the current directory, not in
the directory of the header. The review measured it: `api.h` that
includes a sibling `types.h` binds from the directory of the header,
and fails from another directory, also with an absolute path.

### 265.1 Rules

1. The frontend parses a header with the directory of its source file.
   A quoted include resolves as the platform C compiler resolves it for
   that file: first the directory of the header, then the search path.
2. The basename of the header stays the include spelling that the
   generated mirror records (§23). The parse directory does
   not change the generated text.
3. A header given as text with no file, as in a unit test, keeps the
   current directory as its parse directory.
4. Only declarations of the header file itself are bound, as now. A
   declaration of an included sibling header is not bound, as for a
   system header.

### 265.2 Acceptance

1. Red first, at the pin: a test writes `api.h` and `types.h` in a
   temporary directory, where `api.h` includes `"types.h"` and uses a
   type of it, and runs the bind command from another directory. The
   command fails on the include. The tracking note records the text.
2. After the change, the command succeeds from another directory, with
   a relative and with an absolute path, and the mirror equals the
   mirror made from the directory of the header, byte for byte.
3. A control: a header with a quoted include of a file that does not
   exist fails with the libclang message.
4. The test needs libclang, as the other bind tests do. No corpus entry
   or golden changes.
5. A unit test of the bindgen crate calls `generate_with_options` on a
   header text with a quoted include of a sibling, once with the source
   path and once with no path (rule 3).

### 265.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review measured a relative and an absolute path, the
mirror text, the include spelling, and a missing include. The mirror
and the diagnostics do not hold the path.

1. MINOR, realistic, fixed: no direct unit test called the new option
   `BindOptions::source_path`. Acceptance item 5 adds one.
2. MINOR, realistic, fixed: the test returned early with no check when
   libclang was absent, and no other bind test does. Acceptance item 4
   stated a skip that does not exist.
3. MINOR, contrived, fixed: a comment lost the reason why the name of
   the main file matters.

### 265.4 Open

1. On Windows, a path with an unpaired surrogate cannot become the name
   of the main file.
