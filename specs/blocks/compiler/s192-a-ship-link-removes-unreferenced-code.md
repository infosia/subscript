<!-- §192 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 192. A ship link removes unreferenced code

*(Added 2026-10-11.)* Origin: the text-module measurement round at
`3cc0aae2` (`specs/tracking/text-module-measure.md`). On 2026-10-11
the owner selected this section before the text module.

Problem: the ship tier links a program against
`libsubscript_runtime.a` with no flag that removes unreferenced code.
The linker keeps each archive member that one symbol references, with
all the code in that member. Measured on `aarch64-apple-darwin`, release,
stripped: a program that uses no optional runtime code is 2,841,952
bytes. With `-Wl,-dead_strip` it is 658,352 bytes. So 2.18 MB of each
shipped program is code that the program never calls. The text module
depends on this: without it, a program that does not use the module
carries its tables (+563 KB measured).

### 192.1 Rules

1. **Each ship link removes unreferenced code.** Every link that this
   project runs for the ship tier passes the platform flag: Apple
   `-Wl,-dead_strip`, GNU and LLVM linkers on Linux
   `-Wl,--gc-sections`, MSVC `/OPT:REF`. One function gives the flags
   for a compiler style, and every link site uses it: `build_c_aot`
   (`codegen/src/ship.rs`), `subscript build` (`cli/src/lib.rs`), and
   the test links of `codegen/src/ship_tests.rs`.
2. **The host link.** `subscript link-flags` prints the same flags, so
   a host that links the runtime itself gets the same removal. The C/C++
   tutorial shows them in its link command.
3. **Nothing that is used goes away.** The host exports, the runtime
   entry points that the generated C calls, and the symbols that a
   native library calls stay. A symbol that only a host reaches by
   name at run time (`dlsym`) is not a use case of the ship link; the
   tutorial states that a host that needs one passes its own flag.
4. **No change** to the dev tier, to the runtime source, or to the
   generated C.

### 192.2 Acceptance

1. Every test that ran before passes, with the same counts. Every
   golden stays byte-identical on the ship tier.
2. A test of rule 1: a program that `build_c_aot` links is at most
   half the size of a same-shape control link without the flag. The
   test reads the output of the real link site, not a copy of its
   command. A ratio holds on every platform; an absolute bound does
   not, because an ELF executable keeps the DWARF of its live members.
3. Cost: the link time of one program with and without the flag, and
   the wall time of the codegen tests, against the pin.
4. The tracking note records the size of each example host and of a
   corpus program, before and after.
