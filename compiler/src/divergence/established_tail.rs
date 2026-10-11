//! Established fragment records for later language sections.
use super::DivergenceEntry;

pub(super) const PROVENANCEEMPTYCALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-callback typedef=\"\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A callback record needs a nonempty C typedef identity.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATECALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-callback typedef=\"C\"\n// @subscript-c-callback typedef=\"C\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One callback typedef has one provenance record.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYCALLBACKLIFETIME: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-callback-lifetime aggregate=\"\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A callback lifetime record needs a nonempty aggregate identity.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATECALLBACKLIFETIME: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-callback-lifetime aggregate=\"A\"\n// @subscript-c-callback-lifetime aggregate=\"A\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One callback aggregate has one lifetime record.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYEXTERNALTYPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-external type=\"\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "An external-type record needs a nonempty C type identity.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEEXTERNALTYPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-external type=\"T\"\n// @subscript-c-external type=\"T\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One external type has one provenance record.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYCENUM: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-cenum typedef=\"\" alias=\"E\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A CEnum record needs nonempty typedef and alias identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEPOINTERPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-parameter function=\"f\" parameter=\"p\" const=false\n// @subscript-c-parameter function=\"f\" parameter=\"p\" const=false\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One pointer parameter has one pointer-parameter record.",
                collision: "compiler.md §187",
            };

pub(super) const PROVENANCEEMPTYPOINTERPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-parameter function=\"\" parameter=\"p\" const=false\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A pointer-parameter record names a non-empty function and parameter.",
                collision: "compiler.md §187",
            };

pub(super) const MIRRORMEMBERTARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-member aggregate=\"Missing\" member=\"m\" const=true\ndeclare function foreign():void;\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A member record must name a field of a boundary class of its mirror.",
                collision: "compiler.md §187",
            };

pub(super) const PROVENANCEDUPLICATEMEMBER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-member aggregate=\"A\" member=\"m\" const=true\n// @subscript-c-member aggregate=\"A\" member=\"m\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One boundary-struct member has one member record.",
                collision: "compiler.md §187",
            };

pub(super) const PROVENANCEEMPTYMEMBER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-member aggregate=\"\" member=\"m\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A member record names a non-empty aggregate and member.",
                collision: "compiler.md §187",
            };

pub(super) const PROVENANCEDUPLICATECENUM: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-cenum typedef=\"T\" alias=\"E\"\n// @subscript-c-cenum typedef=\"T\" alias=\"E\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One CEnum typedef has one provenance record.",
                collision: "compiler.md §23.3",
            };

pub(super) const ITERATIONSUBJECTDOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "type Dir = \"north\" | \"south\"; const d: Dir = \"north\"; for (const c of d) { print(c); }",
                subscript: "const d: string = \"north\"; for (const c of d) { print(c); }",
                why: "Iteration requires a declared container or string type; literal unions have no traversal representation.",
                collision: "stdlib.md §14.2",
            };

pub(super) const FIXEDARRAYMETHODS: DivergenceEntry = DivergenceEntry {
                ts: "const xs: FixedArray<i32, 3> = [1, 2, 3]; print(xs.toString());",
                subscript: "const xs: i32[] = [1, 2, 3]; print(xs.toString());",
                why: "FixedArray supports the callback family; other compiler-owned array methods require a dynamic array receiver.",
                collision: "stdlib.md §12",
            };

pub(super) const COMPILEROWNEDVALUE: DivergenceEntry = DivergenceEntry {
                ts: "const held = Array;",
                subscript: "const xs: i32[] = [1]; const copy: i32[] = Array.from(xs);",
                why: "Compiler-owned namespaces and methods lower to direct operations; the language has no value or writable storage for them.",
                collision: "stdlib.md §9.0",
            };

pub(super) const NAMESPACEOBJECTMEMBER: DivergenceEntry = DivergenceEntry {
                ts: "Array.toString();",
                subscript: "const xs: i32[] = [1]; const copy: i32[] = Array.from(xs);",
                why: "Compiler namespaces expose only declared intrinsics; JavaScript prototype members and inherited Object methods have no namespace representation.",
                collision: "stdlib.md §9.0",
            };

pub(super) const UNICODENORMALIZATION: DivergenceEntry = DivergenceEntry {
    ts: "const text: string = \"x\".normalize(\"NFD\");",
    subscript: "const text: string = \"x\".normalize(\"NFC\");",
    why: "Only NFC normalization is available; NFD, NFKC, and NFKD need tables that the runtime does not provide.",
    collision: "compiler.md §193",
};

pub(super) const MATCHOPTIONALINDEX: DivergenceEntry = DivergenceEntry {
                ts: "const hit = \"x\".match(/x/);",
                subscript: "const pattern: RegExp = /x/; if (pattern.test(\"x\")) { const index: i32 = pattern.matchStart(0); }",
                why: "TypeScript makes the match index optional; the language requires a definite i32 index and has no optional numeric field.",
                collision: "stdlib.md §15.3",
            };

pub(super) const ARRAYFLATTENDEPTH: DivergenceEntry = DivergenceEntry {
                ts: "const xs: i32[][] = [[1]]; const flat = xs.flat();",
                subscript: "const xs: i32[][] = [[1]]; const flat: i32[] = []; for (const inner of xs) { for (const value of inner) { flat.push(value); } }",
                why: "A runtime flattening depth cannot determine one static result element type.",
                collision: "stdlib.md §9",
            };

pub(super) const METHODTYPEDOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "const value: i32 = 1; value.toFixed(2);",
                subscript: "const value: i32 = 1; const text: string = (value as f64).toFixed(2);",
                why: "Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds.",
                collision: "stdlib.md §9",
            };

pub(super) const ARRAYJOINDOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "const xs: i32[][] = [[1]]; xs.join();",
                subscript: "const xs: i32[] = [1]; const text: string = xs.join();",
                why: "Array join uses the interpolation rules; nested arrays and other non-interpolatable elements have no implicit string form.",
                collision: "stdlib.md §9",
            };

pub(super) const FIXEDARRAYSPREAD: DivergenceEntry = DivergenceEntry {
                ts: "const xs: i32[] = [1]; const copy: FixedArray<i32, 1> = [...xs];",
                subscript: "const xs: i32[] = [1]; const copy: i32[] = [...xs];",
                why: "Array spread creates a dynamic array; its runtime length cannot construct a FixedArray with a static length.",
                collision: "stdlib.md §14.4",
            };

pub(super) const EXPLICITINTRINSICTYPEARGUMENTS: DivergenceEntry = DivergenceEntry {
                ts: "Context.bytesOf(1);",
                subscript: "const bytes: u8[] = Context.bytesOf<FixedArray<i32, 1>>([1]);",
                why: "The intrinsic requires one explicit type argument for its storage or element type; inferred and mapper overloads do not supply that shape.",
                collision: "stdlib.md §18.1",
            };

pub(super) const SOURCECONSTRUCTIONDOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "const values = new Set<i32>(null);",
                subscript: "const values: Set<i32> = new Set<i32>();",
                why: "Source construction accepts arrays, FixedArray, Set, and string; null and JavaScript array-like objects are outside this domain.",
                collision: "stdlib.md §14",
            };

pub(super) const CALLBACKPARAMETERSHAPE: DivergenceEntry = DivergenceEntry {
                ts: "const xs: i32[] = [1]; xs.map((): i32 => 1);",
                subscript: "const xs: i32[] = [1]; xs.map((value: i32): i32 => value);",
                why: "A container callback declares its element parameters and an optional index; omitted element parameters do not match the runtime callback ABI.",
                collision: "stdlib.md §12",
            };

pub(super) const JSONCALLARGUMENTS: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(1, null);",
                subscript: "const text: string = JSON.stringify(1);",
                why: "JSON intrinsics take one argument; replacer, spacing, and reviver overloads are outside the declared interface.",
                collision: "stdlib.md §13",
            };

pub(super) const JSONTYPEDOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "JSON.stringify(/x/);",
                subscript: "const text: string = JSON.stringify(\"x\");",
                why: "JSON helpers require a supported static data shape; RegExp and container parse targets have no helper representation.",
                collision: "stdlib.md §13",
            };

pub(super) const STRINGSEARCHPATTERN: DivergenceEntry = DivergenceEntry {
                ts: "const index: i32 = \"x\".search(\"x\");",
                subscript: "const index: i32 = \"x\".search(/x/);",
                why: "String search requires a compiled RegExp; implicit conversion from a string pattern is outside the regular-expression interface.",
                collision: "stdlib.md §15.3",
            };

pub(super) const MIRRORPARAMETERPATTERN: DivergenceEntry = DivergenceEntry {
                ts: "declare function first([value]: i32[]): void;",
                subscript: "declare function first(value: i32): void;",
                why: "A mirror function uses named C ABI parameters; parameter destructuring requires a script body that a mirror does not provide.",
                collision: "compiler.md §107",
            };

pub(super) const LOCALENUMBERFORMATTING: DivergenceEntry = DivergenceEntry {
                ts: "const value: f64 = 1.0; value.toLocaleString();",
                subscript: "const value: f64 = 1.0; const text: string = value.toFixed(2);",
                why: "Locale-sensitive number formatting needs host locale data; the runtime provides only explicit locale-independent formats.",
                collision: "stdlib.md §11",
            };

pub(super) const USERITERATIONPROTOCOL: DivergenceEntry = DivergenceEntry {
                ts: "class Bag { items: i32[] = [1, 2]; [Symbol.iterator](): Iterator<i32> { return this.items[Symbol.iterator](); } } for (const value of new Bag()) {}",
                subscript: "const items: i32[] = [1, 2]; for (const value of items) {}",
                why: "User iteration protocols require Symbol.iterator; the runtime traverses only its declared container types.",
                collision: "stdlib.md §14.2",
            };

pub(super) const SETALGEBRADOMAIN: DivergenceEntry = DivergenceEntry {
                ts: "class Values extends Set<i32> {} export function main(): void { const values: Set<i32> = new Set<i32>(); values.union(new Values()); }",
                subscript: "const values: Set<i32> = new Set<i32>(); values.union(new Set<i32>());",
                why: "Set algebra requires a native Set argument; a Set subclass has no runtime container representation.",
                collision: "stdlib.md §14",
            };

pub(super) const VOIDVALUE: DivergenceEntry = DivergenceEntry {
                ts: "function f(): void {} const a = f();",
                subscript: "function f(): void {} f();",
                why: "Call a void function as a statement. Use a bare return in a void function. A map callback must return a value.",
                collision: "C21",
            };

pub(super) const GENERATORDONEVALUE: DivergenceEntry = DivergenceEntry {
                ts: "const b: Box = r.value; const xs: Box[] = [b]; print(`${xs.length}`);",
                subscript: "if (!r.done) { const b: Box = r.value; const xs: Box[] = [b]; print(`${xs.length}`); }",
                why: "A finished generator traps when its zero value contains a non-nullable null reference or a null string handle. Check done before the read.",
                collision: "C23",
            };

pub(super) const REFERENCESEARCHMISS: DivergenceEntry = DivergenceEntry {
                ts: "const missing = values.get(key); print(`${missing === undefined}`);",
                subscript: "const missing = values.get(key); print(`${missing == null}`);",
                why: "JavaScript undefined becomes null for reference search misses and finished nullable-reference generators. Use == null to test absence in both languages.",
                collision: "C22",
            };

pub(super) const LONESURROGATEESCAPE: DivergenceEntry = DivergenceEntry {
    ts: r#"const text: string = "\ud83d";"#,
    subscript: r#"const text: string = "\ud83d\udc4d";"#,
    why: "UTF-8 has no encoding for a lone surrogate. Write the paired escape or the character.",
    collision: "compiler.md §96",
};

pub(super) const ANYTYPE: DivergenceEntry = DivergenceEntry {
    ts: "const value: any = 1;",
    subscript: "const value: i32 = 1;",
    why: "Every declaration must carry a C layout, and `any` carries none, \
                      so no storage can be given to it.",
    collision: "compiler.md §6",
};

pub(super) const DYNAMICOBJECTMODEL: DivergenceEntry = DivergenceEntry {
    ts: "class Greeter { message: string = \"hello\"; }\n\
                     Greeter.prototype.message = \"changed\";\n\
                     eval(\"print(1)\");",
    subscript: "class Greeter { message: string = \"hello\"; }\n\
                            const g: Greeter = new Greeter();\n\
                            g.message = \"changed\";",
    why: "The compiler runs ahead of time and a class lowers to a fixed C \
                      layout, so no code and no member appear at run time.",
    collision: "compiler.md §6",
};

pub(super) const NOMINALCLASSIDENTITY: DivergenceEntry = DivergenceEntry {
    ts: "class A { value: i32 = 1; }\n\
                     class B { value: i32 = 2; }\n\
                     const a: A = new B();",
    subscript: "class A { value: i32 = 1; }\n\
                            const a: A = new A();",
    why: "Each class declaration is one nominal type, so a class with the \
                      same shape is a different type.",
    collision: "C1",
};

pub(super) const OBJECTLITERALCONSTRUCTION: DivergenceEntry = DivergenceEntry {
    ts: "class Shape { value!: i32; }\n\
                     const s: Shape = { value: 1 };",
    subscript: "@Descriptor class Shape { value!: i32; }\n\
                            const s: Shape = { value: 1 };",
    why: "An object literal has no nominal identity, so only a `@Descriptor` \
                      class takes a literal as its construction.",
    collision: "C1",
};

pub(super) const VALUECLASSLAYOUT: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType class Base { value: i32 = 4; }\n\
                     @ValueType class Derived extends Base { extra: i32 = 5; }",
    subscript: "@ValueType class Base { value: i32 = 4; }\n\
                            @ValueType class Derived { base: Base = new Base(); extra: i32 = 5; }",
    why: "A value class lowers to a plain C struct, so it has no base class \
                      and no alignment below its natural one.",
    collision: "C2",
};

pub(super) const BARENUMBER: DivergenceEntry = DivergenceEntry {
    ts: "const count: number = 3;",
    subscript: "const count: i32 = 3;",
    why: "`number` is a 64-bit float with no C width, so every declaration \
                      names one of the sized types.",
    collision: "C3",
};

pub(super) const SIZEDOPERANDWIDTHS: DivergenceEntry = DivergenceEntry {
    ts: "const left: i8 = 1;\n\
                     const right: i16 = 2;\n\
                     const value: i16 = left + right;",
    subscript: "const left: i8 = 1;\n\
                            const right: i16 = 2;\n\
                            const value: i16 = (left as i16) + right;",
    why: "An implicit conversion hides a width change, so every mixed-width \
                      operand and argument takes an explicit `as`.",
    collision: "C3",
};

pub(super) const STORAGEONLYHALFFLOAT: DivergenceEntry = DivergenceEntry {
    ts: "const left: f16 = 1.0;\n\
                     const right: f16 = 2.0;\n\
                     const value: f16 = left + right;",
    subscript: "const left: f16 = 1.0;\n\
                            const right: f16 = 2.0;\n\
                            const value: f16 = ((left as f32) + (right as f32)) as f16;",
    why: "`f16` is a storage format with no portable C arithmetic, so \
                      computation runs in `f32` and converts back.",
    collision: "compiler.md §16",
};

pub(super) const INTEGERLITERALRANGE: DivergenceEntry = DivergenceEntry {
    ts: "const big: i32 = 3000000000;\nconst half: f16 = 70000;",
    subscript: "const big: i64 = 3000000000;",
    why: "A literal takes the sized type of its context, so a value outside \
                      that range has no representation.",
    collision: "C4",
};

pub(super) const WIREENUMVALUES: DivergenceEntry = DivergenceEntry {
    ts: "type Wire = CEnum<{ \"m0\": 1.5 }>;",
    subscript: "type Wire = CEnum<{ \"m0\": 1 }>;",
    why: "A `CEnum` member carries a C constant, so each wire value is a \
                      distinct integer inside the `i32` range.",
    collision: "compiler.md §50",
};

pub(super) const ESCAPINGCAPTURE: DivergenceEntry = DivergenceEntry {
                ts: "function makeAdder(k: i32): (v: i32) => i32 { const captured: i32 = k; return (v: i32): i32 => v + captured; }",
                subscript: "function add(k: i32, v: i32): i32 { return k + v; }",
                why: "A capturing lambda holds its environment on the stack, so it cannot \
                      outlive the function that made it.",
                collision: "C5",
            };

pub(super) const EXCEPTIONS: DivergenceEntry = DivergenceEntry {
    ts: "function fail(): void {\n\
                     \x20 throw \"failure\";\n\
                     }",
    subscript: "function fail(): void {\n\
                            \x20 throw new Error(\"failure\");\n\
                            }",
    why: "The decided surface throws only an Error-family object, reads a catch \
                      binding only through `instanceof` or a rethrow, and has no `finally`.",
    collision: "C6",
};

pub(super) const INSTANCEOFNONERROR: DivergenceEntry = DivergenceEntry {
    ts: "class Box {}\n\
                     const box: Box = new Box();\n\
                     const known: boolean = box instanceof Box;",
    subscript: "no equivalent; the nominal static type already names the class",
    why: "Only an Error-family object carries a runtime class tag, and a nominal \
                      static type already decides every other class.",
    collision: "C6",
};

pub(super) const GENERALUNIONANDUNDEFINED: DivergenceEntry = DivergenceEntry {
    ts: "class Choice { value: i32 | string = 0; }\n\
                     let maybe: i32 | undefined = undefined;",
    subscript: "class Cell { value: i32 = 0; }\n\
                            let maybe: Cell | null = null;",
    why: "A general union has no single C layout, so the one union form is a \
                      nullable reference and `undefined` stays out.",
    collision: "C7",
};

pub(super) const NULLISHNONNULLABLE: DivergenceEntry = DivergenceEntry {
                ts: "class Box {}\nconst a: Box = new Box();\nconst b: Box = a ?? new Box();",
                subscript: "class Box {}\nconst a: Box | null = new Box();\nconst b: Box = a ?? new Box();",
                why: "The nullish test must inspect a nullable pointer, so a non-nullable value has no null branch.",
                collision: "C7",
            };

pub(super) const OPTIONALCHAINNONNULLABLE: DivergenceEntry = DivergenceEntry {
                ts: "class Box { value: i32 = 1; }\nconst a: Box = new Box();\nconst value: i32 = a?.value ?? 0;",
                subscript: "class Box { value: i32 = 1; }\nconst a: Box = new Box();\nconst value: i32 = a.value;",
                why: "The optional test must inspect a nullable pointer, so a non-nullable receiver has no null branch.",
                collision: "C7",
            };

pub(super) const NULLISHASSIGNMENT: DivergenceEntry = DivergenceEntry {
                ts: "class Box {}\nlet a: Box | null = null;\na ??= new Box();",
                subscript: "class Box {}\nlet a: Box | null = null;\nif (a === null) { a = new Box(); }",
                why: "`??=` has no HIR form, so the explicit null test keeps assignment and evaluation order visible.",
                collision: "C7",
            };

pub(super) const NONPLACENULLISHINITIALIZER: DivergenceEntry = DivergenceEntry {
                ts: "class Box {}\nfunction maybe(): Box | null { return null; }\nclass Holder { value: Box = maybe() ?? new Box(); }",
                subscript: "class Box {}\nconst candidate: Box | null = null;\nclass Holder { value: Box = candidate ?? new Box(); }",
                why: "A non-place receiver needs a synthetic local, and an initializer has no statement list that can declare it.",
                collision: "C7",
            };

pub(super) const OPTIONALCHAINUNBOUND: DivergenceEntry = DivergenceEntry {
                ts: "class Box { value: i32 = 1; }\nconst x: Box | null = new Box();\nprint(`${x?.value}`);",
                subscript: "class Box { value: i32 = 1; }\nconst x: Box | null = new Box();\nprint(`${x?.value ?? 0}`);",
                why: "An unbound optional-chain result needs `undefined`, and this language has only `null`.",
                collision: "C7",
            };

pub(super) const OPTIONALCHAININDEX: DivergenceEntry = DivergenceEntry {
                ts: "class Values { [i: u32]: i32; data: i32[] = [1]; get(i: u32): i32 { return this.data[i as i32]; } set(i: u32, v: i32): void { this.data[i as i32] = v; } }
function probe(values: Values | null): void { const value: i32 = values?.[0] ?? 0; }",
                subscript: "class Values { [i: u32]: i32; data: i32[] = [1];\n  get(i: u32): i32 { return this.data[i as i32]; }\n  set(i: u32, value: i32): void { this.data[i as i32] = value; } }\nconst values: Values | null = new Values();\nconst value: i32 = values !== null ? values[0] : 0;",
                why: "Computed optional access is outside the two chain forms that avoid binding `undefined`.",
                collision: "C7",
            };

pub(super) const LITERALUNIONALIAS: DivergenceEntry = DivergenceEntry {
    ts: "type B = \"low\" | \"high\";\n\
                     function f(level: \"low\" | \"high\"): B { return level; }",
    subscript: "type Level = \"low\" | \"high\";\n\
                            function f(level: Level): Level { return level; }",
    why: "A closed literal set is nominal by its alias, so an inline set has \
                      no identity and two aliases stay distinct.",
    collision: "C7",
};

pub(super) const OPTIONALDESCRIPTORMEMBER: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { value?: i32; }\n\
                     const d: D = { value: undefined };\n\
                     print(`${d.value}`);",
    subscript: "type Mode = \"fast\" | \"safe\";
@Descriptor class D { value?: i32 = 1; mode?: Mode; }
const d: D = {};
if (d.mode !== undefined) { print(`${d.mode}`); }",
    why: "An optional member must still hold a value, so it carries a default; \
                      only a closed literal set can be absent.",
    collision: "C7",
};

pub(super) const BOUNDARYONLYOBJECT: DivergenceEntry = DivergenceEntry {
    ts: "function read(value: object): void {}",
    subscript: "class Box { value: i32 = 1; }\n\
                            JSON.stringify(new Box());",
    why: "`object` is the boundary-opaque handle with no field shape, so it is \
                      legal only at the C boundary.",
    collision: "C7",
};

pub(super) const PROMISEOBJECT: DivergenceEntry = DivergenceEntry {
    ts: "const pending = Promise.resolve(1);",
    subscript: "async function leaf(): Promise<i32> { return 1; }
async function probe(): Promise<void> { const value: i32 = await leaf(); }",
    why: "Only async handles and Promise.all over handle arrays exist. Other Promise object operations have no runtime representation.",
    collision: "C8",
};

pub(super) const AWAITOUTSIDEASYNC: DivergenceEntry = DivergenceEntry {
    ts: "await Context.suspend();",
    subscript: "export async function main(): Promise<void> {\n\
                            \x20 await Context.suspend();\n\
                            }",
    why: "Only an async function has the suspendable frame that `await` needs, \
                      so top-level `await` has no frame.",
    collision: "C8",
};

pub(super) const ASYNCFUNCTIONSHAPE: DivergenceEntry = DivergenceEntry {
    ts: "class W { static async work(): Promise<void> {} } const work = async <T>(x: T): Promise<T> => x;",
    subscript: "async function work(x: i32): Promise<i32> { return x; }",
    why: "The async surface excludes static methods, value-class methods, generators, and generic arrows.",
    collision: "C8",
};

pub(super) const DROPPEDASYNCHANDLE: DivergenceEntry = DivergenceEntry {
    ts: "async function work(): Promise<void> { await Context.suspend(); }
function probe(): void { work(); }",
    subscript: "async function work(): Promise<void> { await Context.suspend(); }
async function probe(): Promise<void> { await work(); }",
    why: "No scheduler exists, so an async frame that no holder awaits will \
                      never run to completion.",
    collision: "compiler.md §70",
};

pub(super) const THISINFIELDINITIALIZER: DivergenceEntry = DivergenceEntry {
                ts: "class C { a!: i32; b: i32 = this.a; value: i32 = this.read(); constructor() { this.a = 1; } read(): i32 { return 3; } }\n@Descriptor class D { a?: string = \"x\"; b?: string = this.a + \"x\"; }",
                subscript: "class C { value: i32 = 0; constructor() { this.value = this.read(); } \
                            read(): i32 { return 3; } }",
                why: "Initializers read only earlier initialized fields (§147 rule 2). \
                      Descriptor defaults forbid `this` (§147 rule 3a). Other uses expose the partial instance.",
                collision: "C9",
            };

pub(super) const CLASSINDEXSIGNATURE: DivergenceEntry = DivergenceEntry {
    ts: "class Values { [i: u32]: i32; }\n\
                     const values: Values = new Values();\n\
                     const changed: i32 = values[0] = 2;",
    subscript:
        "class Values { [i: u32]: i32; get(i: u32): i32 { return 0; } set(i: u32, v: i32): void {} }
const values: Values = new Values();
values[0] = 2;
const changed: i32 = values[0];",
    why: "Value-position writes and signatures without declared methods or on value \
                      classes stay out.",
    collision: "C10",
};

pub(super) const USINGDECLARATION: DivergenceEntry = DivergenceEntry {
    ts: "class Resource { [Symbol.dispose](): void {} }
async function probe(): Promise<void> { await using resource = new Resource(); }",
    subscript: "class Resource { [Symbol.dispose](): void {} }
function probe(): void { using resource = new Resource(); }",
    why: "Disposal is synchronous and requires a reference class. Value-class hooks, \
                      descriptor-class hooks, lambda bodies, and `await using` stay out.",
    collision: "C11",
};

pub(super) const NAMEDACCESSOR: DivergenceEntry = DivergenceEntry {
    ts: "class V { v: i32 = 1; get c(): i32 { return this.v; } \
                     set c(x: i32) { this.v = x; } }\n\
                     const a: V = new V();\n\
                     const changed: i32 = a.c = 2;",
    subscript: "class V { v: i32 = 1; get c(): i32 { return this.v; } \
                            set c(x: i32) { this.v = x; } }\n\
                            const a: V = new V();\n\
                            a.c = 2;\n\
                            const changed: i32 = a.c;",
    why: "Value-position writes, value-class write accessors, and mirror accessors \
                      stay out.",
    collision: "C12",
};

pub(super) const ITERATORTEMPORARY: DivergenceEntry = DivergenceEntry {
    ts: "const map: Map<i32, string> = new Map<i32, string>();\n\
                     const keys = map.keys();",
    subscript: "const map: Map<i32, string> = new Map<i32, string>();\n\
                            for (const key of map.keys()) { print(`${key}`); }",
    why: "A held view needs a view type the language does not have \
                      (stdlib.md §14.3).",
    collision: "C13",
};

pub(super) const HOSTAPISURFACE: DivergenceEntry = DivergenceEntry {
    ts: "export function read(): i32 { return 1; }",
    subscript:
        "function read(): i32 { return 1; }\nexport function main(): void { print(`${read()}`); }",
    why: "The entry module exports only functions with supported host boundary signatures.",
    collision: "C18",
};

pub(super) const NAMEDMODULESURFACE: DivergenceEntry = DivergenceEntry {
    ts: "export default function read(): void {}",
    subscript: "export function main(): void {}",
    why: "Named exports keep each module's public surface explicit.",
    collision: "C18",
};

pub(super) const DECLARATIONSCOPE: DivergenceEntry = DivergenceEntry {
    ts: "const outer: i32 = 3;\n\
                     { const read = (): i32 => outer; const outer: i32 = 4; }",
    subscript: "const outer: i32 = 3;\n\
                            { const read = (): i32 => outer; }",
    why: "The two languages resolve the name to different declarations, so this \
                      compiler rejects instead of giving a different value.",
    collision: "C14",
};

pub(super) const MODULEINITIALIZERORDER: DivergenceEntry = DivergenceEntry {
    ts: "class Box {}
const g: Box = f();
function f(): Box { return h; }
const h: Box = new Box();",
    subscript: "class Box {}
const h: Box = new Box();
function f(): Box { return h; }
const g: Box = f();",
    why: "Modules initialize in dependency order and each module in declaration \
                      order, so an initializer must not read a binding that is initialized later.",
    collision: "C14",
};

pub(super) const GROWINGINSTANCECHAIN: DivergenceEntry = DivergenceEntry {
                ts: "function f<T>(x: T): void { f<T[]>([x]); }",
                subscript: "function f<T>(x: T): void { f<T>(x); }",
                why: "This compiler makes one instance per type argument list, so a chain that grows without bound has no finite compiled form.",
                collision: "C19",
            };

pub(super) const STATICMEMBERSURFACE: DivergenceEntry = DivergenceEntry {
    ts: "class Box<T> { static count: i32 = 0; }\n\
                     class C { static value: i32 = 1; static read(): i32 { return this.value; } }",
    subscript: "class C { static value: i32 = 1; static read(): i32 { return C.value; } }",
    why: "Static code has no instance receiver. Generic classes require separate static storage for each type argument list.",
    collision: "compiler.md §71",
};

pub(super) const MATHSUBSET: DivergenceEntry = DivergenceEntry {
    ts: "const m = Math;\n\
                     print(`${Math.max(1, 2, 3)}`);",
    subscript: "print(`${Math.max(Math.max(1, 2), 3)}`);",
    why: "`Math` is a compiler namespace that lowers to intrinsics, so it is \
                      not a value and it takes no variadic call.",
    collision: "stdlib.md §1",
};

pub(super) const DATESUBSET: DivergenceEntry = DivergenceEntry {
    ts: "const d: Date = new Date();\n\
                     const y: i32 = d.getFullYear();\n\
                     print(`now: ${d}`);",
    subscript: "const d: Date = new Date(Date.now());\n\
                            const y: i32 = d.getUTCFullYear();\n\
                            print(`now: ${d.toISOString()}`);",
    why: "The current clock, a local time zone, and a mutable Date make output \
                      that depends on the host.",
    collision: "stdlib.md §3",
};

pub(super) const LOCALESENSITIVESTRING: DivergenceEntry = DivergenceEntry {
                ts: "const s: string = \"a\"; const t: string = s.toLocaleUpperCase(); const r: i32 = s.localeCompare(\"b\");",
                subscript: "const s: string = \"a\"; const t: string = s.toUpperCase(); const same: boolean = s === \"b\";",
                why: "Locale data is host state that changes the result, so only \
                      locale-independent case mapping and equality are in the subset.",
                collision: "stdlib.md §8",
            };

pub(super) const ARRAYMETHODDEFAULTS: DivergenceEntry = DivergenceEntry {
                ts: "const xs: i32[] = [1, 2]; xs.sort();",
                subscript: "const xs: i32[] = [1, 2]; xs.sort((a: i32, b: i32): i32 => a - b); const hit: i32 = xs.findIndex((v: i32): boolean => v > 1); const total: i32 = xs.reduce((a: i32, v: i32): i32 => a + v, 0);",
                why: "The lib's defaults sort as strings, seed from the first element, and \
                      need a miss value that a scalar has not.",
                collision: "stdlib.md §9",
            };

pub(super) const VARIADICARGUMENTS: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = [1, 2]; xs.splice(1, 2, 9, 9, 9); xs.unshift(-1, 0);",
    subscript: "const xs: i32[] = [1, 2]; xs.splice(1, 2); xs.unshift(-1);",
    why: "The language has no variadic parameter, so every call takes a fixed \
                      argument count.",
    collision: "stdlib.md §12",
};

pub(super) const MAPKEYKIND: DivergenceEntry = DivergenceEntry {
    ts: "const map: Map<i32[], i32> = new Map<i32[], i32>();",
    subscript: "const map: Map<i32, i32> = new Map<i32, i32>();",
    why: "A key needs a hash and an equality that the layout gives, so scalars, \
                      strings, and reference handles are the kinds.",
    collision: "stdlib.md §10",
};

pub(super) const MAPSCALARGET: DivergenceEntry = DivergenceEntry {
                ts: "const map: Map<i32, i32> = new Map<i32, i32>(); print(`${map.get(1)}`);",
                subscript: "const map: Map<i32, i32> = new Map<i32, i32>(); if (map.has(1)) { print(`${map.getOr(1, 0)}`); }",
                why: "A scalar has no null miss value, so a lookup is a presence check plus \
                      a defaulted read.",
                collision: "stdlib.md §10",
            };

pub(super) const SHAREDLOCATIONNARROWING: DivergenceEntry = DivergenceEntry {
                ts: "class Child { v: i32 = 1; }
class Holder { c: Child | null = new Child(); }
function clear(h: Holder): void { h.c = null; }
function probe(h: Holder): void { if (h.c !== null) { clear(h); print(`${h.c.v}`); } }",
                subscript: "class Child { v: i32 = 1; }
class Holder { c: Child | null = new Child(); }
function clear(h: Holder): void { h.c = null; }
function probe(h: Holder): void { const c: Child | null = h.c; if (c !== null) { clear(h); print(`${c.v}`); } }",
                why: "Another frame or alias can change a shared location after its null check. Copy the value or narrow again before use.",
                collision: "C17",
            };

pub(super) const MAPNONNULLABLEGET: DivergenceEntry = DivergenceEntry {
                ts: "const map: Map<i32, Generator<i32>> = new Map<i32, Generator<i32>>(); map.get(1);",
                subscript: "function* fallback(): Generator<i32> { yield 1; } const map: Map<i32, Generator<i32>> = new Map<i32, Generator<i32>>(); map.getOr(1, fallback());",
                why: "The value type has no `| null` form of the map's value representation; use a default value.",
                collision: "compiler.md §123",
            };

pub(super) const NOTUPLETYPE: DivergenceEntry = DivergenceEntry {
    ts: "const map: Map<i32, i32> = new Map<i32, i32>([[1, 2]]);\n\
                     for (const entry of map.entries()) { print(`${entry}`); }",
    subscript: "const map: Map<i32, i32> = new Map<i32, i32>();\n\
                            map.set(1, 2);\n\
                            for (const key of map.keys()) { print(`${key}`); }",
    why: "The language has no tuple type, so a pair has no element type to \
                      construct from or to yield.",
    collision: "stdlib.md §14",
};

pub(super) const NUMBERCOERCIONANDARGUMENTS: DivergenceEntry = DivergenceEntry {
    ts: "print(`${isNaN(1.0)}`);\n\
                     const value: f64 = Number(\"1\");\n\
                     print(value.toPrecision());",
    subscript: "print(`${Number.isNaN(1.0)}`);\n\
                            const value: f64 = parseFloat(\"1\");\n\
                            print(value.toPrecision(3));",
    why: "A coercing call reads any run-time type, and an omitted radix or \
                      digit count changes the output silently.",
    collision: "stdlib.md §11",
};

pub(super) const JSONSUBSET: DivergenceEntry = DivergenceEntry {
    ts: "JSON.stringify(new Map<i32, i32>());\n\
                     const parsed = JSON.parse(\"{}\");",
    subscript: "class Box { value: i32 = 1; }\n\
                            print(JSON.stringify(new Box()));\n\
                            const r: Box = JSON.parse<Box>('{\"value\":1}');",
    why: "A container, a function, and a Date have no static field shape, and a \
                      parse needs a declared target type.",
    collision: "stdlib.md §13",
};

pub(super) const AGGREGATELAYOUTLIMIT: DivergenceEntry = DivergenceEntry {
    ts: "const data: FixedArray<u8, 2147483648> = [];",
    subscript: "const data: FixedArray<u8, 4> = [0, 0, 0, 0];",
    why: "A field offset is a signed 32-bit displacement, so one aggregate and \
                      the whole stack frame each have a byte limit.",
    collision: "collisions.md Q29",
};

pub(super) const REGEXPSUBSET: DivergenceEntry = DivergenceEntry {
    ts: "const match = /x/.exec(\"x\");\n\
                     const index: i32 = /x/g.lastIndex;",
    subscript: "const found: boolean = /x/.test(\"x\");\n\
                            print(`${\"x\".replace(/x/, \"y\")}`);",
    why: "An exec result is an array with fields, and `lastIndex` is mutable \
                      global state; the language has neither type.",
    collision: "stdlib.md §15",
};

pub(super) const REPLACEALLGLOBALFLAG: DivergenceEntry = DivergenceEntry {
    ts: "print(\"aaa\".replaceAll(/a/, \"Z\"));",
    subscript: "print(\"aaa\".replaceAll(/a/g, \"Z\"));",
    why: "The lib traps a non-global literal at run time; this compiler reads \
                      the flag, so it reports it at check time.",
    collision: "stdlib.md §15",
};

pub(super) const WORKERENTRYSHAPE: DivergenceEntry = DivergenceEntry {
    ts: "class Message { value: i32 = 0; }\n\
                     async function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): \
                     Promise<void> {}\n\
                     Worker.spawn(entry);",
    subscript: "class Message { value: i32 = 0; }\n\
                            function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): \
                            void {}\n\
                            function run(): void { \
                            const w: Worker<Message, Message> = Worker.spawn(entry); }",
    why: "A worker starts on another thread with its own Context, so its entry \
                      is a named, non-capturing, synchronous module function.",
    collision: "compiler.md §40",
};

pub(super) const WORKERCONTEXTAFFINITY: DivergenceEntry = DivergenceEntry {
                ts: "class Child { value: i32 = 1; }\nclass RefMessage { value: Child = new Child(); }\nfunction entry(inbox: Inbox<RefMessage>, outbox: Outbox<RefMessage>): void {}\nconst worker: Worker<RefMessage, RefMessage> = Worker.spawn(entry);",
                subscript: "class Message { value: i32 = 0; }
function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): void {}
function run(): void { const worker: Worker<Message, Message> = Worker.spawn(entry); }",
                why: "A `string` field is copied by bytes; every other handle stays owned by \
                      one Context.",
                collision: "compiler.md §40",
            };

pub(super) const SWITCHOVERALIAS: DivergenceEntry = DivergenceEntry {
    ts: "type Phase = \"queued\" | \"done\";
function probe(phase: Phase): void { switch (phase) { case \"queued\": break; } }",
    subscript: "type Phase = \"queued\" | \"done\";
function probe(phase: Phase): void { switch (phase) { case \"queued\": break; default: break; } }",
    why: "A closed literal set dispatches on an integer, so every member has one \
                      arm, and no member has two.",
    collision: "compiler.md §41",
};

pub(super) const UNREACHABLEINVALUEPOSITION: DivergenceEntry = DivergenceEntry {
    ts: "const value: i32 = unreachable();",
    subscript: "unreachable();",
    why: "`unreachable()` diverges and gives no value, so it is a statement and \
                      never an operand.",
    collision: "compiler.md §42",
};

pub(super) const DESCRIPTORCONSTRUCTION: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor class D { value?: i32 = 1; }\n\
                     const d: D = new D();",
    subscript: "@Descriptor class D { value?: i32 = 1; }\n\
                            const d: D = { value: 1 };",
    why: "A descriptor class has no constructor and no heap identity, so a \
                      literal in its position constructs it.",
    collision: "compiler.md §25",
};

pub(super) const BYTEACCESSTARGET: DivergenceEntry = DivergenceEntry {
    ts: "class Node { value: i32 = 0; }
const node: Node = new Node();
Context.bytesOf<Node>(node);",
    subscript: "@ValueType class Point { x: i32 = 0; }
const point: Point = new Point();
Context.bytesOf<Point>(point);",
    why: "Storage bytes read only where the layout is C-identical, so a \
                      reference class and a handle field have none.",
    collision: "stdlib.md §18",
};

pub(super) const ENTRYPARAMETERTYPE: DivergenceEntry = DivergenceEntry {
    ts: "type Level = \"low\" | \"high\";\n\
                     export function configure(level: Level): void {}",
    subscript: "type Level = CEnum<{ \"low\": 0; \"high\": 1 }>;\n\
                            export function configure(level: Level): void {}",
    why: "A host-callable entry takes a C type, and a plain literal alias has \
                      no wire representation.",
    collision: "compiler.md §61",
};

pub(super) const EMBEDDEDHEADERCOPY: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts
export function main(): void { const e: Ext = new Ext(new Header(null), 1); const h = e.header; }
// file: mirror.d.ts
// @subscript-c-header include=\"probe.h\"
declare class Header { next: Header|null; constructor(next: Header|null); }
declare class Ext { header: Header; x: i32; constructor(header: Header,x: i32); }",
                subscript: "// file: mirror.d.ts
// @subscript-c-header include=\"probe.h\"
declare class Header { next: Header|null; constructor(next: Header|null); }
declare class Ext { header: Header; x: i32; constructor(header: Header,x: i32); }
// file: main.ts
export function main(): void { const e: Ext = new Ext(new Header(null), 1); const h: Header | null = e.header; }",
                why: "A copy carries the extension's tag with no extension behind it, so \
                      the host reads past the header.",
                collision: "compiler.md §33.5",
            };

pub(super) const GENERICINFERENCECANDIDATES: DivergenceEntry = DivergenceEntry {
                ts: "function pair<T>(a: T, b: T): T { return a; }\nconst n: i32 = 1; const f: f64 = 2.5; pair(n, f);",
                subscript: "function pair<T>(a: T, b: T): T { return a; }\nconst n: i32 = 1; const f: f64 = 2.5; pair<f64>(n as f64, f);",
                why: "Inference needs one candidate type for each parameter. Supply explicit type arguments when candidates conflict.",
                collision: "compiler.md §149",
            };

pub(super) const GENERICINFERENCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "function empty<T>(): T | null { return null; }\nempty();",
                subscript: "class Item {}
function empty<T>(): T | null { return null; }
empty<Item>();",
                why: "Inference needs an argument candidate for each type parameter. Supply explicit type arguments when no argument gives a candidate.",
                collision: "compiler.md §149",
            };

pub(super) const GENERICMETHODTYPEARGUMENTS: DivergenceEntry = DivergenceEntry {
                ts: "class Box { identity<T>(value: T): T { return value; } }\n\
                     const box: Box = new Box();\n\
                     print(`${box.identity(1)}`);",
                subscript: "class Box { identity<T>(value: T): T { return value; } }\n\
                            const box: Box = new Box();\n\
                            print(`${box.identity<i32>(1)}`);",
                why: "Each explicit type-argument list names one method instance. Generic method calls do not infer type arguments.",
                collision: "compiler.md §64",
            };

pub(super) const BODILESSDECLAREGENERICMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "declare class Box { identity<T>(value: T): T; }",
    subscript: "class Box { identity<T>(value: T): T { return value; } }",
    why: "A template must carry the body that each explicit type-argument list instantiates.",
    collision: "compiler.md §64",
};

pub(super) const GENERICMETHODONGENERICCLASS: DivergenceEntry = DivergenceEntry {
    ts: "class Holder<T> { value: T;\n\
                       constructor(value: T) { this.value = value; }\n\
                       pick<U>(other: U): U { return other; } }",
    subscript: "class Holder<T> { value: T;\n\
                              constructor(value: T) { this.value = value; } }\n\
                            function pick<U>(other: U): U { return other; }",
    why: "The checker holds one substitution, so a class parameter and a \
                      method parameter cannot bind at the same time.",
    collision: "compiler.md §64",
};

pub(super) const GENERATORSINGLEUSE: DivergenceEntry = DivergenceEntry {
    ts: "function* one(): Generator<i32> { yield 1; }\n\
                     const values: i32[] = [...one()];",
    subscript: "function* one(): Generator<i32> { yield 1; }\n\
                            const values: i32[] = [];\n\
                            for (const value of one()) { values.push(value); }",
    why: "A generator is single-use, so consuming it reads as a value \
                      expression while it mutates the generator.",
    collision: "stdlib.md §14.4",
};

pub(super) const BAREMAPSUBJECT: DivergenceEntry = DivergenceEntry {
    ts: "const map: Map<i32, string> = new Map<i32, string>();\n\
                     for (const entry of map) { print(`${entry}`); }",
    subscript: "const map: Map<i32, string> = new Map<i32, string>();\n\
                            for (const key of map.keys()) { print(`${key}`); }",
    why: "TypeScript binds a `[K, V]` pair here and this language binds `K`, \
                      so an accepted program fails the `tsc` gate.",
    collision: "compiler.md §104",
};

pub(super) const BAREMAPTOARRAY: DivergenceEntry = DivergenceEntry {
    ts: "const map: Map<i32, string> = new Map<i32, string>();\n\
                     const entries = [...map];",
    subscript: "const map: Map<i32, string> = new Map<i32, string>();\n\
                            const keys: i32[] = [];\n\
                            for (const key of map.keys()) { keys.push(key); }",
    why: "TypeScript reads a `Map` element as a `[K, V]` pair and this language \
                      reads `K`, so an accepted program fails the `tsc` gate.",
    collision: "compiler.md §104",
};

pub(super) const ARRAYFROMMAPPER: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = [1, 2];\n\
                     const doubled = Array.from(xs, (value: i32): i32 => value * 2);",
    subscript: "const xs: i32[] = [1, 2];\n\
                            const doubled: i32[] = [];\n\
                            for (const value of xs) { doubled.push(value * 2); }",
    why: "The mapper overload needs callback typing and traversal work, and that \
                      cost is not measured.",
    collision: "compiler.md §105.2",
};

pub(super) const ARRAYISARRAY: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = [1, 2];\n\
                     const flag: boolean = Array.isArray(xs);",
    subscript: "no equivalent; a declared type already answers it",
    why: "A declared type answers this statically, and the runtime classification \
                      a boundary-opaque value needs is not inspected.",
    collision: "compiler.md §105.3",
};

pub(super) const ARRAYOFARITY: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = Array.of<i32>(1, 2);",
    subscript: "const xs: i32[] = [1, 2];",
    why: "Variable arity needs the variadic-parameter prerequisite.",
    collision: "compiler.md §105.3",
};

pub(super) const ARRAYHOLECONSTRUCTION: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const xs: i32[] = new Array<i32>(3); const hole = [,1]; const spread = [,...[1]]; }",
                subscript: "const xs: i32[] = [];\n\
                            for (let index: i32 = 0; index < 3; index = index + 1) { \
                            xs.push(0); }",
                why: "The language has no array hole and no missing-element value, so a \
                      filled array changes what a read means.",
                collision: "compiler.md §105.3",
            };

pub(super) const PATTERNDEFAULTVALUE: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
const xs: i32[] = [];
const [first = 1] = xs;
}",
    subscript: "const xs: i32[] = [];\n\
                            const first: i32 = xs.length > 0 ? xs[0] : 1;",
    why: "A default fires on a missing element, which TypeScript reads as \
                      `undefined`; this language has no `undefined`.",
    collision: "compiler.md §107.3",
};

pub(super) const ARRAYRESTPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
const xs: i32[] = [1, 2, 3];
const [head, ...rest] = xs;
}",
    subscript: "const xs: i32[] = [1, 2, 3];\n\
                            const head: i32 = xs[0];\n\
                            const rest: i32[] = xs.slice(1);",
    why: "A rest element needs allocation and copy semantics for a second array, \
                      which this section does not decide.",
    collision: "compiler.md §107.3",
};

pub(super) const OBJECTRESTPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
class Point { x: i32 = 1; y: i32 = 2; }
const { x, ...rest } = new Point();
}",
    subscript: "class Point { x: i32 = 1; y: i32 = 2; }\n\
                            function read(): i32 {\n\
                            \x20 const point: Point = new Point();\n\
                            \x20 const x: i32 = point.x;\n\
                            \x20 return x;\n\
                            }",
    why: "A field rest needs a result shape and property-selection rules, and the \
                      language has no object type.",
    collision: "compiler.md §107.3",
};

pub(super) const NESTEDPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
const xss: i32[][] = [[1, 2]];
const [[first, second]] = xss;
}",
    subscript: "function read(): i32 {\n\
                            \x20 const xss: i32[][] = [[1, 2]];\n\
                            \x20 const [inner] = xss;\n\
                            \x20 const [first, second] = inner;\n\
                            \x20 return first + second;\n\
                            }",
    why: "A pattern inside a pattern needs recursive type checks and an order for \
                      its effects.",
    collision: "compiler.md §107.3",
};

pub(super) const PATTERNFIELDNAME: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
class Point { x: i32 = 1; }
const key = \"x\" as const;
const { [key]: value } = new Point();
}",
    subscript: "class Point { x: i32 = 1; }\n\
                            function read(): i32 {\n\
                            \x20 const { x: value } = new Point();\n\
                            \x20 return value;\n\
                            }",
    why: "A field name is resolved at compile time, so a computed key names no \
                      field.",
    collision: "compiler.md §107.1",
};

pub(super) const PATTERNSOURCESHAPE: DivergenceEntry = DivergenceEntry {
    ts: "function probe(): void {
const text = \"ab\";
const [first, second] = text;
}",
    subscript: "no equivalent; a binding pattern reads a `T[]`, a \
                            `FixedArray<T, N>`, or a class instance",
    why: "A binding pattern reads an array by index, or a class by field name, so \
                      another source shape has no pattern.",
    collision: "compiler.md §107.1",
};

pub(super) const ASSIGNMENTPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = [1, 2];\n\
                     let first: i32 = 0;\n\
                     [first] = xs;",
    subscript: "const xs: i32[] = [1, 2];\n\
                            let first: i32 = 0;\n\
                            first = xs[0];",
    why: "A pattern binds new names; a pattern that writes existing targets needs \
                      an evaluation and write order.",
    collision: "compiler.md §107.3",
};

pub(super) const MODULELEVELPATTERN: DivergenceEntry = DivergenceEntry {
    ts: "const xs: i32[] = [1, 2];\n\
                     const [first, second] = xs;",
    subscript: "const xs: i32[] = [1, 2];\n\
                            const first: i32 = xs[0];\n\
                            const second: i32 = xs[1];",
    why: "A module-level name carries a declared type, and a pattern gives each \
                      name the type its read answers.",
    collision: "compiler.md §107.1",
};

pub(super) const DEFINITEASSIGNMENTASSERTION: DivergenceEntry = DivergenceEntry {
    ts: "class Inner { v: i32 = 3; }\n\
                     class Holder { inner!: Inner; }",
    subscript: "class Inner { v: i32 = 3; }\n\
                            class Holder {\n\
                            \x20 inner: Inner;\n\
                            \x20 constructor(inner: Inner) { this.inner = inner; }\n\
                            }",
    why: "The assertion asks `tsc` to trust the author; this language has no null \
                      check on a non-nullable reference.",
    collision: "compiler.md §108",
};

pub(super) const NESTEDFIELDASSIGNMENTEVERYNORMALEXIT: DivergenceEntry = DivergenceEntry {
    ts: "class Holder {\n\
                     \x20 x: i32;\n\
                     \x20 constructor(flag: boolean) {\n\
                     \x20   if (flag) { this.x = 1; } else { this.x = 2; }\n\
                     \x20 }\n\
                     }",
    subscript: "class Holder {\n\
                            \x20 x: i32 = 2;\n\
                            \x20 constructor(flag: boolean) {\n\
                            \x20   if (flag) { this.x = 1; }\n\
                            \x20 }\n\
                            }",
    why: "The rule reads the constructor's top level only; a definite-assignment \
                      analysis is a larger change than a field needs.",
    collision: "compiler.md §108",
};

pub(super) const AMBIENTCLASSCONSTRUCTION: DivergenceEntry = DivergenceEntry {
    ts: "declare class Ext { value: i32; }\n\
                     const ext: Ext = new Ext();",
    subscript: "no equivalent; declare the class in a `.d.ts` mirror, and take \
                            the instance from a `declare function` there",
    why: "A `declare class` has no constructor body, so `new` stores no argument \
                      and every field of the instance holds no value.",
    collision: "compiler.md §108",
};

pub(super) const THISBEFOREFIELDVALUES: DivergenceEntry = DivergenceEntry {
                ts: "class Inner {}
class Holder { inner: Inner; constructor() { this.show(); this.inner = new Inner(); } show(): void {} }",
                subscript: "class Inner {}
class Holder { inner: Inner; constructor() { this.inner = new Inner(); this.show(); } show(): void {} }",
                why: "The callee can read a field that holds no value, and the \
                      definite-assignment analysis of `tsc` does not follow a call.",
                collision: "compiler.md §108",
            };
