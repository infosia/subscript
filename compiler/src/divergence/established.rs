//! Established divergence fragment records.
use super::DivergenceEntry;

pub(super) const NULLINITIALIZERINFERENCE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const x=null; }",
                subscript: "class C {}\nexport function main(): void { const x: C | null = null; }",
                why: "A bare null initializer supplies no inferred type, so the declaration needs an explicit nullable annotation.",
                collision: "compiler.md §97",
            };

pub(super) const USINGBINDINGRESOURCETYPE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { using x:null=null; }",
                subscript: "class R { [Symbol.dispose](): void {} }\nexport function main(): void { using x: R | null = null; }",
                why: "A using binding must name a reference resource class with a disposal hook, optionally nullable.",
                collision: "compiler.md §97.1",
            };

pub(super) const MIRROREXPORTLIST: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare const x_exports_73=1; export {x_exports_73};",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare const x_exports_73 = 1;",
                why: "A mirror declares the ambient C surface directly and cannot declare an export list.",
                collision: "compiler.md §128.1",
            };

pub(super) const TOPLEVELNAMECLASH: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: i32): void; function f(x: i32): void {}",
                subscript: "function f(x: i32): void {}",
                why: "One declaration owns each top-level name in a module; a second declaration cannot add an overload or a merged declaration.",
                collision: "compiler.md §125.1",
            };

pub(super) const UNSUPPORTEDMODULEDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as N from \"./other\"; import A = N.A;\n// file: other.ts\nexport class A {}",
                subscript: "// file: main.ts\nimport { A } from \"./other\";\n// file: other.ts\nexport class A {}",
                why: "The module surface uses export declarations, named export lists, named imports, and namespace imports only.",
                collision: "C18",
            };

pub(super) const POISONEDDEFAULTIMPORT: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport def from \"s154_fragment_poison_default\";\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare module \"s154_fragment_poison_default\" {const value_fragment_poison_default:i32;export default value_fragment_poison_default;}",
                subscript: "// file: main.ts\nimport { def } from \"./other\";\n// file: other.ts\nexport const def: i32 = 1;",
                why: "A default import stays outside the named module surface, even when module discovery marks the source absent.",
                collision: "C18",
            };

pub(super) const DEFAULTIMPORT: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport def, * as ns from \"./other\"; ns.default();\n// file: other.ts\nexport default function f(): void {}",
                subscript: "// file: main.ts\nimport {f} from \"./other\"; f();\n// file: other.ts\nexport function f(): void {}",
                why: "A default import stays outside the named module surface.",
                collision: "C18",
            };

pub(super) const COMPUTEDMETHODNAME: DivergenceEntry = DivergenceEntry {
                ts: "class A { [\"f\"](): void {} }\nexport function main(): void {}",
                subscript: "class A { f(): void {} }",
                why: "Only the disposal hook has a computed method name; every other computed method name is rejected.",
                collision: "compiler.md §60.1",
            };

pub(super) const FUNCTIONBODYMISSING: DivergenceEntry = DivergenceEntry {
    ts: "declare function f():void;\nexport function main(): void {  }",
    subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function f(): void;",
    why: "A source function needs a body; an ambient function belongs in a host mirror.",
    collision: "compiler.md §108.1",
};

pub(super) const STATICFIELDINITIALIZERMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A {static x:i32;}\nexport function main(): void {  }",
    subscript: "class A { static x: i32 = 0; }",
    why: "A static field needs an initializer because its module storage must start with a value.",
    collision: "compiler.md §108.1",
};

pub(super) const FIELDASSIGNMENTAFTERUNREACHABLERETURN: DivergenceEntry = DivergenceEntry {
                ts: "class A {x:i32;constructor(){if(false)return;this.x=1;}}\nexport function main(): void {  }",
                subscript: "class A { x: i32; constructor() { this.x = 1; if (false) return; } }",
                why: "A field assignment counts only before every constructor statement that contains a return.",
                collision: "compiler.md §108.1",
            };

pub(super) const FIELDASSIGNMENTMISSINGNONORMALEXIT: DivergenceEntry = DivergenceEntry {
                ts: "class A {x:i32;constructor(){throw new Error(\"x\");}}\nexport function main(): void {  }",
                subscript: "class A { x: i32 = 0; constructor() { throw new Error(\"x\"); } }",
                why: "Every ordinary instance field needs an initializer or a top-level constructor assignment, even when the constructor always throws.",
                collision: "compiler.md §108.1",
            };

pub(super) const CONSTRUCTORDEFINITEFIELDREADBEFOREASSIGNMENT: DivergenceEntry = DivergenceEntry {
                ts: "class A {x!:i32;constructor(){print(`${this.x}`);this.x=1;}}\nexport function main(): void {  }",
                subscript: "class A { x: i32; constructor() { this.x = 1; print(`${this.x}`); } }",
                why: "A constructor reads a field only after an initializer or an earlier top-level statement gives it a value.",
                collision: "compiler.md §108.4",
            };

pub(super) const FOROFAWAITUSING: DivergenceEntry = DivergenceEntry {
                ts: "async function f():Promise<void> { for(await using x of [null]) {} }\nexport function main(): void {  }",
                subscript: "class R { [Symbol.dispose](): void {} }\nasync function f(): Promise<void> { for (const value of [new R()]) { using x = value; } }",
                why: "Disposal hooks run synchronously; an await using loop binding is outside this disposal surface.",
                collision: "compiler.md §60.1",
            };

pub(super) const ALIASCASENONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "type A=\"a\"|\"b\";\nexport function main(): void { const x:A=\"a\";let y:A=\"a\";switch(x){case y:break;} }",
                subscript: "type A = \"a\" | \"b\";\nexport function main(): void { const x: A = \"a\"; switch (x) { case \"a\": break; default: break; } }",
                why: "An alias-valued case label requires a member literal or an explicit conversion to string.",
                collision: "compiler.md §321",
            };

pub(super) const CLASSMEMBERNAMECLASH: DivergenceEntry = DivergenceEntry {
    ts: "class A { f(v:i32):void; f(v:i32):void {} }\nexport function main(): void {  }",
    subscript: "class A { f(v: i32): void {} }",
    why: "One field, method, or accessor pair owns each name in its class member namespace.",
    collision: "compiler.md §65.1",
};

pub(super) const DESCRIPTORMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor\nclass A { get x():i32 {return 1;} }\nexport function main(): void {  }",
    subscript: "@Descriptor class A { x!: i32; }\nfunction getX(a: A): i32 { return a.x; }",
    why: "A descriptor class contains data only and declares no method or accessor.",
    collision: "compiler.md §25.1",
};

pub(super) const MIRRORSTATICMETHOD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_162 { static f():void; }",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function f(): void;",
                why: "A mirror class describes C instance storage and declares no static method or accessor.",
                collision: "compiler.md §71.1",
            };

pub(super) const DISPOSESTATIC: DivergenceEntry = DivergenceEntry {
    ts: "class A { static [Symbol.dispose]():void {} }\nexport function main(): void {  }",
    subscript: "class A { [Symbol.dispose](): void {} }",
    why: "A disposal hook must be an instance method.",
    collision: "compiler.md §60.1",
};

pub(super) const MIRRORACCESSOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_197 { get x():i32; }",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function getX(): i32;",
                why: "A mirror accessor has no C field representation; a foreign function can expose the read operation.",
                collision: "compiler.md §65.1",
            };

pub(super) const READACCESSORRETURNMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x() {return 1;} }\nexport function main(): void {  }",
    subscript: "class A { get x(): i32 { return 1; } }",
    why: "A read accessor must declare an explicit return type.",
    collision: "compiler.md §65.1",
};

pub(super) const WRITEACCESSORPATTERN: DivergenceEntry = DivergenceEntry {
    ts:
        "class A { get x():i32 {return 1;} set x([v]:i32[]) {} }\nexport function main(): void {  }",
    subscript: "class A { get x(): i32 { return 1; } set x(v: i32) {} }",
    why: "A write accessor must declare one named parameter with an explicit type.",
    collision: "compiler.md §65.1",
};

pub(super) const WRITEACCESSORTYPEMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x():i32 {return 1;} set x(v) {} }\nexport function main(): void {  }",
    subscript: "class A { get x(): i32 { return 1; } set x(v: i32) {} }",
    why: "A write accessor must declare an explicit parameter type.",
    collision: "compiler.md §65.1",
};

pub(super) const GENERICMETHODBODYMISSING: DivergenceEntry = DivergenceEntry {
                ts: "class A { f<T>(v:T):T; f<T>(v:T):T {return v;} }\nexport function main(): void {  }",
                subscript: "class A { f<T>(v: T): T { return v; } }",
                why: "A generic method template needs a body at collection; a separate overload signature has no template body.",
                collision: "compiler.md §82.4",
            };

pub(super) const DISPOSEASYNC: DivergenceEntry = DivergenceEntry {
    ts: "class A { async [Symbol.dispose]():Promise<void> {} }\nexport function main(): void {  }",
    subscript: "class A { [Symbol.dispose](): void {} }",
    why: "A disposal hook must run synchronously.",
    collision: "compiler.md §60.1",
};

pub(super) const DISPOSESIGNATURE: DivergenceEntry = DivergenceEntry {
    ts: "class A { [Symbol.dispose](v:i32):void {} }\nexport function main(): void {  }",
    subscript: "class A { [Symbol.dispose](): void {} }",
    why: "A disposal hook takes no parameter and returns void.",
    collision: "compiler.md §60.1",
};

pub(super) const DESCRIPTORINHERITANCE: DivergenceEntry = DivergenceEntry {
    ts: "class Base {}\n@Descriptor\nclass A extends Base {}\nexport function main(): void {  }",
    subscript: "@Descriptor class A {}",
    why: "A descriptor class declares its own data and does not inherit.",
    collision: "compiler.md §25.1",
};

pub(super) const REFERENCECLASSINHERITANCE: DivergenceEntry = DivergenceEntry {
                ts: "class Base {}\n\nclass A extends Base { constructor() { super(); } }\nexport function main(): void {  }",
                subscript: "class Base {}\nclass A { base: Base = new Base(); }",
                why: "A reference class has its own nominal identity and C layout; the language has no class inheritance.",
                collision: "compiler.md §115.1",
            };

pub(super) const DESCRIPTORSTATICFIELD: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor\nclass A { static x:i32=1; }\nexport function main(): void {  }",
    subscript: "const x: i32 = 1;\n@Descriptor class A {}",
    why: "A descriptor class declares instance data only and has no static field.",
    collision: "compiler.md §71.1",
};

pub(super) const DESCRIPTORPRIVATENAME: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor\nclass A { #x!: i32; }\nexport function main(): void {  }",
    subscript: "@Descriptor class A { x!: i32; }",
    why: "A descriptor member is a public data member that a literal fills by name.",
    collision: "compiler.md §206",
};

pub(super) const MIRRORPRIVATENAME: DivergenceEntry = DivergenceEntry {
    ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_private_name { #x: i32; }",
    subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare class A_private_name { x: i32; }",
    why: "A mirror class member names a member of a C struct, and C has no private member.",
    collision: "compiler.md §206",
};

pub(super) const JSONPARSEPRIVATENAME: DivergenceEntry = DivergenceEntry {
    ts: "class C { #x: i32 = 1; }\nexport function main(): void { JSON.parse<C>(\"{}\"); }",
    subscript:
        "class C { private x: i32 = 1; }\nexport function main(): void { JSON.parse<C>(\"{}\"); }",
    why: "`JSON.stringify` omits a private name, so a document has no key that sets it.",
    collision: "compiler.md §206",
};

pub(super) const MIRRORSTATICFIELD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_493 { static x:i32; }",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function getX(): i32;",
                why: "A mirror class describes C instance storage and has no static field.",
                collision: "compiler.md §71.1",
            };

pub(super) const STATICFIELDOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "class A { static x?:i32; }\nexport function main(): void {  }",
                subscript: "class Cell { value: i32 = 0; }\nclass A { static x: Cell | null = null; }",
                why: "A static field cannot represent undefined; optional fields belong only to the descriptor surface.",
                collision: "C7",
            };

pub(super) const CONTEXTAFFINESTATICFIELD: DivergenceEntry = DivergenceEntry {
                ts: "class A { static x:Inbox<A>; }\nexport function main(): void {  }",
                subscript: "class A {}\nfunction f(x: Inbox<A>): void { x; }",
                why: "Worker, Inbox, and Outbox values belong to one Context and cannot enter static field storage.",
                collision: "compiler.md §40",
            };

pub(super) const DESCRIPTORINITIALIZERWITHOUTOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { x:i32=1; }\nexport function main(): void {  }",
                subscript: "@Descriptor class A { x?: i32 = 1; }",
                why: "A descriptor member with a default initializer must use the optional question-mark spelling.",
                collision: "compiler.md §25.1",
            };

pub(super) const DESCRIPTORREQUIREDWITHOUTDEFINITE: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { x:i32;constructor(){this.x=1;} }\nexport function main(): void {  }",
                subscript: "@Descriptor class A { x!: i32; }",
                why: "A required descriptor member must use the definite-assignment spelling so a constructing literal supplies its value.",
                collision: "compiler.md §25.1",
            };

pub(super) const INSTANCEFIELDOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "class A { x?:i32; }\nexport function main(): void {  }",
                subscript: "class Cell { value: i32 = 0; }\nclass A { x: Cell | null = null; }",
                why: "An ordinary instance field cannot represent undefined; optional fields belong only to the descriptor surface.",
                collision: "C7",
            };

pub(super) const WIREALIASNESTEDFIELD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_class_shape_624=CEnum<{a:1}>; declare class A_class_shape_624 { x:FixedArray<W_class_shape_624,2>; }",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W_class_shape_624 = CEnum<{ a: 1 }>; declare class A_class_shape_624 { x: W_class_shape_624; }",
                why: "A wire alias occupies a direct boundary field or an array-pair element; a nested boundary type has no declared wire position.",
                collision: "compiler.md §52.2",
            };

pub(super) const CONTEXTAFFINEINSTANCEFIELD: DivergenceEntry = DivergenceEntry {
                ts: "class A { x!:Inbox<A>; }\nexport function main(): void {  }",
                subscript: "class A {}\nfunction f(x: Inbox<A>): void { x; }",
                why: "Worker, Inbox, and Outbox values belong to one Context and cannot enter instance field storage.",
                collision: "compiler.md §40",
            };

pub(super) const VALUEFIELDOUTSIDEWHITELIST: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType\nclass A { x:string=\"x\"; }\nexport function main(): void {  }",
                subscript: "class A { x: string = \"x\"; }",
                why: "A value class contains only sized numerics, booleans, value classes, fixed arrays, enums, and admitted literal aliases.",
                collision: "C2",
            };

pub(super) const DESCRIPTORCONSTRUCTOR: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { constructor() {} }\nexport function main(): void {  }",
                subscript: "@Descriptor class A {}\nconst a: A = {};",
                why: "A descriptor class receives its fields from a constructing literal and declares no constructor.",
                collision: "compiler.md §25.1",
            };

pub(super) const WIREALIASNESTEDCONSTRUCTORPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_class_shape_719=CEnum<{a:1}>; declare class A_class_shape_719 { constructor(x:FixedArray<W_class_shape_719,2>); }",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W_class_shape_719 = CEnum<{ a: 1 }>; declare class A_class_shape_719 { constructor(x: W_class_shape_719); }",
                why: "A wire alias is a direct mirror constructor parameter or an array-pair element; other nested positions have no wire representation.",
                collision: "compiler.md §52.2",
            };

pub(super) const CLASSINDEXSIGNATURECOUNT: DivergenceEntry = DivergenceEntry {
    ts: "class A { [a:i32]:i32; [b:string]:i32; }\nexport function main(): void {  }",
    subscript:
        "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } set(a: i32, value: i32): void {} }",
    why: "A class declares at most one index signature for its accessor pair.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATURENONREFERENCE: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType\nclass A { [a:i32]:i32; }\nexport function main(): void {  }",
    subscript:
        "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } set(a: i32, value: i32): void {} }",
    why: "Only an ordinary reference class can declare an index signature.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATURESTATIC: DivergenceEntry = DivergenceEntry {
    ts: "class A { static [a:i32]:i32; }\nexport function main(): void {  }",
    subscript:
        "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } set(a: i32, value: i32): void {} }",
    why: "A class index signature describes instance access through its get and set methods.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATUREINDEXTYPE: DivergenceEntry = DivergenceEntry {
    ts: "class A { [a:string]:i32; }\nexport function main(): void {  }",
    subscript:
        "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } set(a: i32, value: i32): void {} }",
    why: "A class index signature requires an i32 or u32 index.",
    collision: "compiler.md §58.1",
};

pub(super) const WRITEACCESSORWITHOUTREAD: DivergenceEntry = DivergenceEntry {
                ts: "class C { set x(v: i32) {} static set y(v: i32) {} }\nexport function main(): void { const c = new C(); c.x; c.x = 1; C.y; C.y = 1; }",
                subscript: "class C { get x(): i32 { return 0; } set x(v: i32) {} static get y(): i32 { return 0; } static set y(v: i32) {} }",
                why: "A write accessor needs a read accessor because property writes use the shared accessor type.",
                collision: "compiler.md §65.1",
            };

pub(super) const ACCESSORTYPEMISMATCH: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x():i32 {return 1;} set x(v:string) {} }\nexport function main(): void {  }",
    subscript: "class A { get x(): i32 { return 1; } set x(v: i32) {} }",
    why: "A read accessor and its write accessor must share exactly one type.",
    collision: "compiler.md §65.1",
};

pub(super) const CLASSINDEXSETSIGNATURE: DivergenceEntry = DivergenceEntry {
                ts: "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } }",
                subscript: "class A { [a: i32]: i32; get(a: i32): i32 { return 0; } set(a: i32, value: i32): void {} }",
                why: "A mutable class index signature needs a synchronous set method with exactly matching index and element types.",
                collision: "compiler.md §58.1",
            };

pub(super) const MODULEUSING: DivergenceEntry = DivergenceEntry {
                ts: "using x = { [Symbol.dispose]():void {} };\nexport function main(): void {  }",
                subscript: "class R { [Symbol.dispose](): void {} }\nexport function main(): void { using x = new R(); }",
                why: "A using binding needs a function block scope for deterministic disposal; module bindings have no such exit.",
                collision: "compiler.md §60.1",
            };

pub(super) const DESCRIPTOROPTIONS: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function Descriptor(options: Target): (target: Target) => void { return (target: Target): void => {}; } @Descriptor({}) class A {}",
                subscript: "@Descriptor class A {}",
                why: "The Descriptor decorator accepts no options.",
                collision: "compiler.md §62.1",
            };

pub(super) const UNSUPPORTEDCLASSDECORATOR: DivergenceEntry = DivergenceEntry {
    ts: "class Target {} function other(target: Target): void {} @other class A {}",
    subscript: "class A {}",
    why: "Only the ambient ValueType and Descriptor decorators define a class representation.",
    collision: "compiler.md §130.1",
};

pub(super) const DESCRIPTORVALUETYPE: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType\n@Descriptor\nclass A {}\nexport function main(): void {  }",
    subscript: "@Descriptor class A {}",
    why: "A descriptor is a reference class and cannot also declare value-class representation.",
    collision: "compiler.md §25.1",
};

pub(super) const ENUMIMPLICITVALUEOVERFLOW: DivergenceEntry = DivergenceEntry {
    ts: "enum E { a=2147483647,b }\nexport function main(): void {  }",
    subscript:
        "no equivalent: The implicit value 2147483648 cannot fit the i32 enum representation.",
    why: "An enum member occupies i32 storage, so an implicit next value must fit the i32 range.",
    collision: "compiler.md §72",
};

pub(super) const WIREENUMEMPTY: DivergenceEntry = DivergenceEntry {
    ts: "type A = CEnum<{}>;\nexport function main(): void {  }",
    subscript: "no equivalent: A CEnum requires at least one member with a C wire value.",
    why: "A CEnum wire mapping must contain at least one member.",
    collision: "compiler.md §50.1",
};

pub(super) const WIREENUMMEMBERFORM: DivergenceEntry = DivergenceEntry {
                ts: "type A=CEnum<{[key:string]:number}>;\nexport function main(): void {  }",
                subscript: "type A = CEnum<{ a: 1 }>;",
                why: "A CEnum mapping consists of named properties whose types are integer literals, rather than methods or index signatures.",
                collision: "compiler.md §50.1",
            };

pub(super) const WIREENUMMEMBERKEY: DivergenceEntry = DivergenceEntry {
                ts: "type A = CEnum<{ 1:1 }>;\nexport function main(): void {  }",
                subscript: "type A = CEnum<{ \"1\": 1 }>;",
                why: "A CEnum member key must name a string-literal union member, so numeric property keys are excluded.",
                collision: "compiler.md §50.1",
            };

pub(super) const MIRRORVARIABLEFORM: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare const A_declarations_818:i32;",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare const A_declarations_818 = 1;",
                why: "A mirror binds constants with non-negative integer-literal values; it does not bind a host data symbol.",
                collision: "compiler.md §136.1",
            };

pub(super) const WIREALIASNESTEDFOREIGNPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport {};\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W = CEnum<{a: 1}>; declare function f(x: FixedArray<W, 2>): void;",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W_signatures_47 = CEnum<{ a: 1 }>; declare function f_signatures_47(x: W_signatures_47): void;",
                why: "A wire alias is a direct foreign parameter or an array-pair element; another nested position has no declared wire representation.",
                collision: "compiler.md §52.2",
            };

pub(super) const WIREALIASNESTEDFOREIGNRETURN: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport {};\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W = CEnum<{a: 1}>; declare function f(): W[];",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ntype W_signatures_60 = CEnum<{ a: 1 }>; declare function f_signatures_60(): W_signatures_60;",
                why: "A wire alias has a direct foreign return representation only; an array return has no declared wire representation.",
                collision: "compiler.md §50.2",
            };

pub(super) const FOREIGNDIRECTCALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport {};\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function f(cb: (x: i32) => void): void;",
                subscript: "no equivalent: A direct foreign callback parameter has no C provenance position; its C API needs a mirrored callback struct.",
                why: "A callback occupies a mirrored boundary-struct field; a direct foreign callback parameter has no declared provenance position.",
                collision: "compiler.md §23.3",
            };

pub(super) const FOREIGNRETURNPROVENANCE: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport {};\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare function f(): string;",
                subscript: "no equivalent: A foreign string return has no C return provenance; its C API needs a mirrored output struct.",
                why: "A foreign string-view, descriptor, or callback return has no return provenance in the boundary vocabulary.",
                collision: "compiler.md §23.3",
            };

pub(super) const FOREIGNRESULTREAD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport {};\n// file: mirror.d.ts\n// @subscript-c-header include=\"probe.h\"\ndeclare class L { tag: i32; items: u32[]; constructor(tag: i32, items: u32[]); } declare function f(): L;",
                subscript: "// file: main.ts\nexport function main(): void { const p: P = f_result_plain(); print(`${p.tag}`); }\n// file: mirror.d.ts\n// @subscript-c-header include=\"result-read.h\"\ndeclare class P { tag: i32; count: u32; constructor(tag: i32, count: u32); }\ndeclare function f_result_plain(): P;",
                why: "C writes a struct the script reads; no read lowering exists for its pair, view, callback, descriptor, userdata, or validated-pair member (§187).",
                collision: "compiler.md §187",
            };

pub(super) const FOREIGNSTRUCTCYCLE: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\n// @subscript-c-header include=\"struct-cycle.h\"\ndeclare class N { v: i32; next: N | null; constructor(v: i32, next: N | null); }\ndeclare function f_cycle(n: N | null): void;",
                subscript: "// file: main.ts\nexport function main(): void { print(`${f_sum(new N(1, null))}`); }\n// file: mirror.d.ts\n// @subscript-c-header include=\"struct-cycle.h\"\n// @subscript-c-member aggregate=\"N\" member=\"next\" const=true\n// @subscript-c-parameter function=\"f_sum\" parameter=\"n\" const=true\ndeclare class N { v: i32; next: N | null; constructor(v: i32, next: N | null); }\ndeclare function f_sum(n: N | null): i32;",
                why: "A call builds each scratch copy once; a struct that reaches itself through pointers it must copy has no finite scratch build (§187 rule 9).",
                collision: "compiler.md §187",
            };

pub(super) const ASYNCGENERATORFUNCTION: DivergenceEntry = DivergenceEntry {
                ts: "async function *f():AsyncGenerator<i32> {yield 1;}\nexport function main(): void {  }",
                subscript: "no equivalent: The runtime has no async iterator or async generator frame.",
                why: "An async function uses a Promise<T> return view; an async generator needs an async iterator return view.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNANNOTATIONMISSING: DivergenceEntry = DivergenceEntry {
                ts: "async function f() {}\nexport function main(): void {  }",
                subscript: "async function f(): Promise<void> {}",
                why: "An async function must declare its suspendable return view with an explicit Promise<T> annotation.",
                collision: "compiler.md §26.1",
            };

pub(super) const OPTIONALPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "function f(x?:i32):void {}\nfunction apply(cb:(x?:i32)=>void):void { cb(); }\nexport function main(): void {  }",
    subscript: "function f(x: i32 = 0): void {}\nfunction apply(cb: (x: i32) => void): void { cb(0); }",
    why: "An optional parameter implies undefined, which has no language value.",
    collision: "C7",
};

pub(super) const ERRORMESSAGETYPE: DivergenceEntry = DivergenceEntry {
                ts: "type Message=\"a\"|\"b\";\nexport function main(): void { const message:Message=\"a\";const e=new Error(message); }",
                subscript: "type Message = \"a\" | \"b\";\nexport function main(): void { const message: Message = \"a\"; const e = new Error(`${message}`); }",
                why: "An Error message must be a string; a nominal literal alias needs explicit formatting.",
                collision: "compiler.md §115.1",
            };

pub(super) const ERRORCONSTRUCTORARGUMENTS: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const e=new Error(\"x\",{cause:1}); }",
                subscript: "export function main(): void { const e = new Error(\"x\"); }",
                why: "An Error constructor accepts an optional string message only; options and spread arguments are outside its surface.",
                collision: "compiler.md §115.1",
            };

pub(super) const ERRORCALLWITHOUTNEW: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const e=Error(\"x\"); }",
    subscript: "export function main(): void { const e = new Error(\"x\"); }",
    why: "An Error-family object must use the new constructor spelling.",
    collision: "compiler.md §115.1",
};

pub(super) const VALUETYPEARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType() class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "The ValueType decorator requires exactly one object-literal argument without a spread.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONSNONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType(1) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "The ValueType decorator requires an object-literal options argument.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONCOUNT: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType options literal must contain exactly the align property.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONSPREAD: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({...{align: 2}}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType options literal declares align directly and does not spread properties.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONPROPERTYFORM: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({align() {}}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType options literal declares align as a key-value property.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONKEY: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({other: 2}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType options literal accepts only the align key.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEALIGNMENTNONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({align: \"2\"}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType alignment must be a numeric integer literal.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEALIGNMENTOUTSIDESET: DivergenceEntry = DivergenceEntry {
                ts: "class Target {} function ValueType(options: Target = new Target()): (target: Target) => void { return (target: Target): void => {}; } @ValueType({align: 3}) class A {}",
                subscript: "@ValueType({ align: 16 }) class A {}",
                why: "A ValueType alignment must be one of 2, 4, 8, or 16.",
                collision: "compiler.md §62.1",
            };

pub(super) const INVALIDPROGRAMENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: a.ts\nexport function read(): void {}\n// file: b.ts\nexport function write(): void {}",
                subscript: "no equivalent: The build input must select one non-ambient entry module.",
                why: "A program with multiple source files must name exactly one non-ambient entry module in its build input.",
                collision: "compiler.md §129.1",
            };

pub(super) const ASYNCRETURNNONREFERENCE: DivergenceEntry = DivergenceEntry {
    ts: "async function f(): (Promise<void>) {}\nexport function main(): void {}",
    subscript: "async function f(): Promise<void> {}",
    why: "An async function must spell its suspendable return view as Promise<T>.",
    collision: "compiler.md §26.1",
};

pub(super) const ASYNCRETURNQUALIFIEDNAME: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): globalThis.Promise<void> {}\nexport function main(): void {}",
                subscript: "async function f(): Promise<void> {}",
                why: "An async function must spell its suspendable return view with the unqualified Promise<T> name.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNALIAS: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"alias.h\"\ntype P = Promise<i32>;\n// file: main.ts\nexport async function f(): P { return 1; }\n",
                subscript: "async function f(): Promise<i32> { return 1; }",
                why: "An async function must spell its suspendable return view as Promise<T>, rather than an alias.",
                collision: "compiler.md §26.1",
            };

pub(super) const FIXEDARRAYLENGTHRANGE: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: FixedArray<u8, 4294967296>): void {}\nexport function main(): void {}",
                subscript: "no equivalent: The requested fixed storage exceeds the signed aggregate byte limit.",
                why: "A FixedArray must fit the signed aggregate byte limit, which excludes lengths above the element-count range.",
                collision: "collisions.md Q29",
            };

pub(super) const FIXEDARRAYLENGTHLITERAL: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: FixedArray<u8, -1>): void {}\nexport function main(): void {}",
    subscript: "no equivalent: A C array cannot have a negative length.",
    why: "A C array layout needs a non-negative integer literal length at compile time.",
    collision: "collisions.md Q3",
};

pub(super) const FIXEDARRAYLENGTHZERO: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: FixedArray<i32, 0>): void {}\nexport function main(): void {}",
    subscript: "function f(x: i32[]): void {}",
    why: "C has no array with the length zero, so a FixedArray length is at least 1 (invariant 1).",
    collision: "collisions.md Q3",
};

pub(super) const GENERICCONSTRAINTIDENTITY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32 = 1; } class D { x: i32 = 1; } function f<T extends C>(x:T): void {} export function main(): void { f<D>(new D()); }",
                subscript: "class C { x: i32 = 1; } function f<T extends C>(x: T): void {}\nexport function main(): void { f<C>(new C()); }",
                why: "A generic constraint uses nominal class identity, rather than TypeScript structural compatibility.",
                collision: "C1",
            };

pub(super) const NAMESPACEUNEXPORTEDMEMBER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport def, * as ns from \"./other\"; ns.default();\n// file: other.ts\nexport default function f(): void {}",
                subscript: "// file: main.ts\nimport {f} from \"./other\"; f();\n// file: other.ts\nexport function f(): void {}",
                why: "A namespace qualifier exposes only the declared named exports; a default export stays outside the module surface.",
                collision: "C18",
            };

pub(super) const RUNNERMAINMISSING: DivergenceEntry = DivergenceEntry {
                ts: "export function update(): void {}",
                subscript: "export function update(): void {}\nexport function main(): void { update(); }",
                why: "The corpus runner calls the entry module host function main; a library export cannot supply that entry.",
                collision: "collisions.md Q12",
            };

pub(super) const CLASSFINALALIGNMENTLIMIT: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType({align:16}) class C { x:FixedArray<u8,2147483647>; constructor(x:FixedArray<u8,2147483647>){this.x=x;} }\nexport function main(): void {}",
                subscript: "no equivalent: The requested aligned layout exceeds the signed displacement limit.",
                why: "Final alignment contributes bytes to the object layout and must stay within the signed displacement limit.",
                collision: "collisions.md Q29",
            };

pub(super) const AGGREGATEARGUMENTFRAMELIMIT: DivergenceEntry = DivergenceEntry {
                ts: "function consume(a:FixedArray<u8,1100000000>,b:FixedArray<u8,1100000000>):void{} function probe(a:FixedArray<u8,1100000000>,b:FixedArray<u8,1100000000>):void{consume(a,b);} export function main():void{}",
                subscript: "class Block { data: FixedArray<u8, 1100000000>; constructor(data: FixedArray<u8, 1100000000>) { this.data = data; } }\nfunction consume(a: Block, b: Block): void {} function probe(a: Block, b: Block): void { consume(a, b); }",
                why: "Argument copies occupy stack storage and must stay within the accumulated frame limit.",
                collision: "collisions.md Q29",
            };

pub(super) const NAMESPACEASVALUE: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as ns from \"./other\"; class C {} const typed: C = ns; export function probe(): void { function ns(): void {} ns(); }\n// file: other.ts\nexport function f(): void {}",
                subscript: "no equivalent: A namespace import has no runtime object value.",
                why: "A namespace import is a static qualifier and has no runtime value.",
                collision: "C18",
            };

pub(super) const SWITCHCASEREAD: DivergenceEntry = DivergenceEntry {
                ts: "export function main():void {const n:i32=1; switch(n){case 0:let x:i32=1;break;case 1:const f:()=>i32=():i32=>x;print(`${f()}`);break;}}",
                subscript: "export function main(): void { const n: i32 = 1; const x: i32 = 1; switch (n) { case 0: break; case 1: const f = (): i32 => x; print(`${f()}`); break; } }",
                why: "A closure can run before the initializer of a binding in another case.",
                collision: "C14",
            };

pub(super) const BLOCKNAMEREADBEFOREDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "class C {} export function main(): void { const f:()=>i32=():i32=>C; const C:i32=1; }",
                subscript: "class C {} export function main(): void { const C: i32 = 1; const f = (): i32 => C; }",
                why: "A block owns each declared name throughout its scope; an earlier read must not resolve to an outer declaration.",
                collision: "C14",
            };

pub(super) const BLOCKNAMEWRITEBEFOREDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "export function main():void { const f:()=>void=():void=>{x=2;}; let x:i32=1; f(); }",
                subscript: "class Cell { x: i32 = 1; }\nexport function main(): void { const cell = new Cell(); const f = (): void => { cell.x = 2; }; f(); }",
                why: "A block owns each declared name throughout its scope; an earlier write must not target an outer declaration.",
                collision: "C14",
            };

pub(super) const CONTEXTAFFINECAPTURE: DivergenceEntry = DivergenceEntry {
                ts: "class M {} function f(box: Inbox<M>): void { const read:()=>void=():void=>{print(`${box}`);}; read(); }\nexport function main(): void {}",
                subscript: "no equivalent: A worker endpoint has no closure-environment representation or string formatter.",
                why: "A worker handle belongs to one Context and cannot enter a closure environment.",
                collision: "compiler.md §40",
            };

pub(super) const MUTABLELOCALCAPTURE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { let x:i32=1; const f:()=>i32=():i32=>x; print(`${f()}`); }",
                subscript: "class Cell { x: i32 = 1; }\nexport function main(): void { const cell = new Cell(); const f = (): i32 => cell.x; print(`${f()}`); }",
                why: "A closure copies const local values; mutable captures need shared storage that the language does not provide.",
                collision: "C5",
            };

pub(super) const CONTEXTAFFINEARRAYELEMENT: DivergenceEntry = DivergenceEntry {
    ts: "class M {} function f(x:Inbox<M>[]):void {}\nexport function main(): void {}",
    subscript: "no equivalent: Worker endpoints cannot enter array storage.",
    why: "A worker handle belongs to one Context and cannot enter array storage.",
    collision: "compiler.md §40",
};

pub(super) const CONTEXTAFFINECONTAINERARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "class M {} function f(x:Map<i32,Inbox<M>>):void {}\nexport function main(): void {}",
    subscript: "no equivalent: Worker endpoints cannot enter container storage.",
    why: "A worker handle belongs to one Context and cannot enter container storage.",
    collision: "compiler.md §40",
};

pub(super) const WORKERMESSAGEPLAINCLASS: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class M { x: i32 = 1; } function f(x: Worker<M, M>): void {}",
                subscript: "class M { x: i32 = 1; } function f(x: Worker<M, M>): void {}",
                why: "Worker messaging copies a plain reference class; scalar and decorated message types have no declared transfer shape.",
                collision: "stdlib.md §16.2",
            };

pub(super) const BOUNDARYLITERALALIAS: DivergenceEntry = DivergenceEntry {
                ts: "// file: boundary-alias.ts\nexport function main(): void {}\n// file: boundary-alias.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare type Choice=\"a\"|\"b\";\ndeclare function foreign(x:Choice):void;",
                subscript: "// file: main.ts\nexport function main(): void {}\n// file: mirror.d.ts\n// @subscript-c-header include=\"x.h\"\ntype Choice = CEnum<{ a: 0; b: 1 }>; declare function foreign(x: Choice): void;",
                why: "A string-literal alias has no C wire representation for a mirror signature.",
                collision: "compiler.md §24.1",
            };

pub(super) const NULLABLENONREFERENCE: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: i32 | null): void {}\nexport function main(): void {}",
                subscript: "class Cell { value: i32 = 0; }\nfunction f(x: Cell | null): void {}",
                why: "Only reference classes, opaque handles, function types, boundary pointers, strings, and arrays with no counted element have a nullable representation.",
                collision: "C7",
            };

pub(super) const NULLABLEVALUECLASSASSIGNMENT: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C {} interface C {(): i32;} function f(c: C): void { const fn: (() => i32) | null = c; }",
                subscript: "no equivalent: A value class has no callable identity or nullable function representation.",
                why: "A value class stores its fields inline and has no nullable pointer representation.",
                collision: "C7",
            };

pub(super) const MIRRORHEADERMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-header.ts\nexport function main(): void {}\n// file: prov-header.d.ts\ndeclare function foreign(x:i32):void;",
                subscript: "no equivalent: Foreign functions need a header identity for C emission. Generate the mirror again from its C header.",
                why: "Foreign functions need a header identity for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORPARAMETERTARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-parameter.ts\nexport function main(): void {}\n// file: prov-target-parameter.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-string-view function=\"missing\" parameter=\"x\" aggregate=\"View\"\ndeclare function foreign():void;",
                subscript: "no equivalent: Parameter provenance must name a parameter declared in the same mirror. Generate the mirror again from its C header.",
                why: "Parameter provenance must name a parameter declared in the same mirror.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORCALLBACKTARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-callback.ts\nexport function main(): void {}\n// file: prov-target-callback.d.ts\n// @subscript-c-callback typedef=\"Missing\" aggregate=\"SubStringView\" data=\"data\" len=\"len\"",
                subscript: "no equivalent: Callback provenance must name a callback typedef declared in the same mirror. Generate the mirror again from its C header.",
                why: "Callback provenance must name a callback typedef declared in the same mirror.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORLIFETIMETARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-lifetime.ts\nexport function main(): void {}\n// file: prov-target-lifetime.d.ts\n// @subscript-c-callback-lifetime aggregate=\"Missing\"",
                subscript: "no equivalent: A callback lifetime record must name a boundary class that carries a callback field. Generate the mirror again from its C header.",
                why: "A callback lifetime record must name a boundary class that carries a callback field.",
                collision: "compiler.md §111",
            };

pub(super) const MIRRORARRAYPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-array.ts\nexport function main(): void {}\n// file: prov-array.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign(xs:i32[]):void;",
                subscript: "no equivalent: An absorbed array parameter needs its C aggregate or scalar-pair provenance for C emission. Generate the mirror again from its C header.",
                why: "An absorbed array parameter needs its C aggregate or scalar-pair provenance for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORSTRINGPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-string.ts\nexport function main(): void {}\n// file: prov-string.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign(x:string):void;",
                subscript: "no equivalent: An absorbed string parameter needs its C string-view aggregate identity for C emission. Generate the mirror again from its C header.",
                why: "An absorbed string parameter needs its C string-view aggregate identity for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORPARAMETERPROVENANCEMISMATCH: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-incompatible.ts\nexport function main(): void {}\n// file: prov-incompatible.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-string-view function=\"foreign\" parameter=\"x\" aggregate=\"View\"\ndeclare function foreign(x:i32):void;",
                subscript: "no equivalent: The declared parameter type must agree with its recorded C parameter shape. Generate the mirror again from its C header.",
                why: "The declared parameter type must agree with its recorded C parameter shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORANONYMOUSCALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: callback-anonymous-field.ts\nexport function main():void {}\n// file: callback-anonymous-field.d.ts\ndeclare class C {cb:(x:i32)=>void;}",
                subscript: "no equivalent: A callback needs a named C typedef for its trampoline cast. Generate the mirror again from its C header.",
                why: "A callback needs a named C typedef for its trampoline cast.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORCALLBACKPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: callback-alias-field.ts\nexport function main():void {}\n// file: callback-alias-field.d.ts\ndeclare type Callback=(x:i32)=>void; declare class C {cb:Callback;}",
                subscript: "no equivalent: A callback needs its recorded C typedef identity for its trampoline cast. Generate the mirror again from its C header.",
                why: "A callback needs its recorded C typedef identity for its trampoline cast.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNKNOWNKIND: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-unknown\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance record must use a declared record kind. Generate the mirror again from its C header.",
                why: "A provenance record must use a declared record kind.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEMISSINGKIND: DivergenceEntry = DivergenceEntry {
    ts: "// file: mirror.d.ts\n// @subscript-c-\n// file: main.ts\nexport function main():void{}",
    subscript: "no equivalent: A provenance record needs a record kind. Generate the mirror again from its C header.",
    why: "A provenance record needs a record kind.",
    collision: "compiler.md §23.3",
};

pub(super) const PROVENANCEFIELDSEPARATOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\"parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: Whitespace must separate provenance fields. Generate the mirror again from its C header.",
                why: "Whitespace must separate provenance fields.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNEXPECTEDKEY: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header wrong=\"a.h\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: Each provenance field must use the key required by its record kind. Generate the mirror again from its C header.",
                why: "Each provenance field must use the key required by its record kind.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNQUOTEDSTRING: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=a.h\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance string must use quotes. Generate the mirror again from its C header.",
                why: "A provenance string must use quotes.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNTERMINATEDSTRING: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance string must have a closing quote. Generate the mirror again from its C header.",
                why: "A provenance string must have a closing quote.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNTERMINATEDESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\\\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance escape must contain its escaped character. Generate the mirror again from its C header.",
                why: "A provenance escape must contain its escaped character.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNSUPPORTEDESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\z\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance string must use a declared escape form. Generate the mirror again from its C header.",
                why: "A provenance string must use a declared escape form.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCECONTROLCHARACTER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\x01b\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance string must escape control characters. Generate the mirror again from its C header.",
                why: "A provenance string must escape control characters.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDUNICODEDIGITS: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\uGGGG\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A Unicode escape must contain four hexadecimal digits. Generate the mirror again from its C header.",
                why: "A Unicode escape must contain four hexadecimal digits.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDBOOLEAN: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=other\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance boolean must use true or false. Generate the mirror again from its C header.",
                why: "A provenance boolean must use true or false.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCETRAILINGDATA: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a.h\" extra\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance record must end after its declared fields. Generate the mirror again from its C header.",
                why: "A provenance record must end after its declared fields.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCESHORTUNICODEESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\u12\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A Unicode escape must contain four hexadecimal digits. Generate the mirror again from its C header.",
                why: "A Unicode escape must contain four hexadecimal digits.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDUNICODESCALAR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\uD800\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A provenance string must contain Unicode scalar values. Generate the mirror again from its C header.",
                why: "A provenance string must contain Unicode scalar values.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEHEADERBASENAME: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-malformed.ts\nexport function main(): void {}\n// file: prov-malformed.d.ts\n// @subscript-c-header include=\"bad/path.h\"\ndeclare function foreign():void;",
                subscript: "no equivalent: A header identity must be a nonempty basename without control characters. Generate the mirror again from its C header.",
                why: "A header identity must be a nonempty basename without control characters.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEHEADER: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-duplicate.ts\nexport function main(): void {}\n// file: prov-duplicate.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign():void;",
                subscript: "no equivalent: A mirror has one header identity. Generate the mirror again from its C header.",
                why: "A mirror has one header identity.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYDESCRIPTOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A descriptor record needs nonempty function, parameter, aggregate, and element identities. Generate the mirror again from its C header.",
                why: "A descriptor record needs nonempty function, parameter, aggregate, and element identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEDESCRIPTOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: One parameter has one recorded C shape. Generate the mirror again from its C header.",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYSTRINGVIEW: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-string-view function=\"\" parameter=\"p\" aggregate=\"A\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A string-view record needs nonempty function, parameter, and aggregate identities. Generate the mirror again from its C header.",
                why: "A string-view record needs nonempty function, parameter, and aggregate identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATESTRINGVIEW: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-string-view function=\"f\" parameter=\"p\" aggregate=\"A\"\n// @subscript-c-string-view function=\"f\" parameter=\"p\" aggregate=\"A\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: One parameter has one recorded C shape. Generate the mirror again from its C header.",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYSCALARPAIR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-scalar-pair function=\"\" parameter=\"p\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: A scalar-pair record needs nonempty function, parameter, and element identities. Generate the mirror again from its C header.",
                why: "A scalar-pair record needs nonempty function, parameter, and element identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATESCALARPAIR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-scalar-pair function=\"f\" parameter=\"p\" element=\"E\" const=true\n// @subscript-c-scalar-pair function=\"f\" parameter=\"p\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent: One parameter has one recorded C shape. Generate the mirror again from its C header.",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };
