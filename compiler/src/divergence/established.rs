//! Established divergence fragment records.
use super::DivergenceEntry;

pub(super) const NULLINITIALIZERINFERENCE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const x=null; }",
                subscript: "no equivalent; annotate the declaration with a nullable reference type",
                why: "A bare null initializer supplies no inferred type, so the declaration needs an explicit nullable annotation.",
                collision: "compiler.md §97",
            };

pub(super) const USINGBINDINGRESOURCETYPE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { using x:null=null; }",
                subscript: "no equivalent; annotate the binding with a resource class or its nullable type",
                why: "A using binding must name a reference resource class with a disposal hook, optionally nullable.",
                collision: "compiler.md §97.1",
            };

pub(super) const MIRROREXPORTLIST: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare const x_exports_73=1; export {x_exports_73};",
                subscript: "no equivalent; declare the host names directly in the mirror",
                why: "A mirror declares the ambient C surface directly and cannot declare an export list.",
                collision: "compiler.md §128.1",
            };

pub(super) const TOPLEVELNAMECLASH: DivergenceEntry = DivergenceEntry {
                ts: "function f(x:i32):void; function f(x:i32):void {}\nexport function main(): void {  }",
                subscript: "no equivalent; declare one implementation for each module name",
                why: "One declaration owns each top-level name in a module; a second declaration cannot add an overload or a merged declaration.",
                collision: "compiler.md §125.1",
            };

pub(super) const UNSUPPORTEDMODULEDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "namespace N {export class A {}} import A=N.A;\nexport function main(): void {  }",
                subscript: "no equivalent; use a named export or a named import",
                why: "The module surface uses export declarations, named export lists, named imports, and namespace imports only.",
                collision: "C18",
            };

pub(super) const POISONEDDEFAULTIMPORT: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport def from \"s154_fragment_poison_default\";\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare module \"s154_fragment_poison_default\" {const value_fragment_poison_default:i32;export default value_fragment_poison_default;}",
                subscript: "no equivalent; use a named import",
                why: "A default import stays outside the named module surface, even when module discovery marks the source absent.",
                collision: "C18",
            };

pub(super) const DEFAULTIMPORT: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport def from \"./other\";\nexport function main(): void {  }\n// file: other.ts\nexport default 1;",
                subscript: "no equivalent; use a named import",
                why: "A default import stays outside the named module surface.",
                collision: "C18",
            };

pub(super) const COMPUTEDMETHODNAME: DivergenceEntry = DivergenceEntry {
                ts: "class A { [\"f\"](): void {} }\nexport function main(): void {}",
                subscript: "no equivalent; declare the method with an identifier name",
                why: "Only the disposal hook has a computed method name; every other computed method name is rejected.",
                collision: "compiler.md §60.1",
            };

pub(super) const FUNCTIONBODYMISSING: DivergenceEntry = DivergenceEntry {
    ts: "declare function f():void;\nexport function main(): void {  }",
    subscript: "no equivalent; move the ambient function declaration to a host mirror",
    why: "A source function needs a body; an ambient function belongs in a host mirror.",
    collision: "compiler.md §108.1",
};

pub(super) const STATICFIELDINITIALIZERMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A {static x:i32;}\nexport function main(): void {  }",
    subscript: "no equivalent; initialize the static field",
    why: "A static field needs an initializer because its module storage must start with a value.",
    collision: "compiler.md §108.1",
};

pub(super) const FIELDASSIGNMENTAFTERUNREACHABLERETURN: DivergenceEntry = DivergenceEntry {
                ts: "class A {x:i32;constructor(){if(false)return;this.x=1;}}\nexport function main(): void {  }",
                subscript: "no equivalent; move the field assignment before every statement that contains a return",
                why: "A field assignment counts only before every constructor statement that contains a return.",
                collision: "compiler.md §108.1",
            };

pub(super) const FIELDASSIGNMENTMISSINGNONORMALEXIT: DivergenceEntry = DivergenceEntry {
                ts: "class A {x:i32;constructor(){throw new Error(\"x\");}}\nexport function main(): void {  }",
                subscript: "no equivalent; give the field an initializer",
                why: "Every ordinary instance field needs an initializer or a top-level constructor assignment, even when the constructor always throws.",
                collision: "compiler.md §108.1",
            };

pub(super) const CONSTRUCTORDEFINITEFIELDREADBEFOREASSIGNMENT: DivergenceEntry = DivergenceEntry {
                ts: "class A {x!:i32;constructor(){print(`${this.x}`);this.x=1;}}\nexport function main(): void {  }",
                subscript: "no equivalent; move the read after a top-level assignment of the field",
                why: "A constructor reads a field only after an initializer or an earlier top-level statement gives it a value.",
                collision: "compiler.md §108.4",
            };

pub(super) const FOROFAWAITUSING: DivergenceEntry = DivergenceEntry {
                ts: "async function f():Promise<void> { for(await using x of [null]) {} }\nexport function main(): void {  }",
                subscript: "no equivalent; use a synchronous using binding",
                why: "Disposal hooks run synchronously; an await using loop binding is outside this disposal surface.",
                collision: "compiler.md §60.1",
            };

pub(super) const ALIASCASENONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "type A=\"a\"|\"b\";\nexport function main(): void { const x:A=\"a\";const y:A=\"a\";switch(x){case y:break;} }",
                subscript: "no equivalent; use a member string literal for each case label",
                why: "A switch over a string-literal alias requires each case label to spell a member literal.",
                collision: "compiler.md §41",
            };

pub(super) const CLASSMEMBERNAMECLASH: DivergenceEntry = DivergenceEntry {
    ts: "class A { f(v:i32):void; f(v:i32):void {} }\nexport function main(): void {  }",
    subscript: "no equivalent; give each method or field a distinct name",
    why: "One field, method, or accessor pair owns each name in its class member namespace.",
    collision: "compiler.md §65.1",
};

pub(super) const DESCRIPTORMETHOD: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor\nclass A { get x():i32 {return 1;} }\nexport function main(): void {  }",
    subscript: "no equivalent; move the behavior to a function",
    why: "A descriptor class contains data only and declares no method or accessor.",
    collision: "compiler.md §25.1",
};

pub(super) const MIRRORSTATICMETHOD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_162 { static f():void; }",
                subscript: "no equivalent; declare a foreign function for the host operation",
                why: "A mirror class describes C instance storage and declares no static method or accessor.",
                collision: "compiler.md §71.1",
            };

pub(super) const DISPOSESTATIC: DivergenceEntry = DivergenceEntry {
    ts: "class A { static [Symbol.dispose]():void {} }\nexport function main(): void {  }",
    subscript: "no equivalent; declare a non-static disposal hook",
    why: "A disposal hook must be an instance method.",
    collision: "compiler.md §60.1",
};

pub(super) const MIRRORACCESSOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_197 { get x():i32; }",
                subscript: "no equivalent; declare a foreign function for the host operation",
                why: "A mirror class reads C fields and does not declare a script accessor.",
                collision: "compiler.md §65.1",
            };

pub(super) const READACCESSORRETURNMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x() {return 1;} }\nexport function main(): void {  }",
    subscript: "no equivalent; annotate the read accessor return type",
    why: "A read accessor must declare an explicit return type.",
    collision: "compiler.md §65.1",
};

pub(super) const WRITEACCESSORPATTERN: DivergenceEntry = DivergenceEntry {
    ts:
        "class A { get x():i32 {return 1;} set x([v]:i32[]) {} }\nexport function main(): void {  }",
    subscript: "no equivalent; use one identifier parameter with an explicit type",
    why: "A write accessor must declare one named parameter with an explicit type.",
    collision: "compiler.md §65.1",
};

pub(super) const WRITEACCESSORTYPEMISSING: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x():i32 {return 1;} set x(v) {} }\nexport function main(): void {  }",
    subscript: "no equivalent; annotate the write accessor parameter",
    why: "A write accessor must declare an explicit parameter type.",
    collision: "compiler.md §65.1",
};

pub(super) const GENERICMETHODBODYMISSING: DivergenceEntry = DivergenceEntry {
                ts: "class A { f<T>(v:T):T; f<T>(v:T):T {return v;} }\nexport function main(): void {  }",
                subscript: "no equivalent; declare one generic method with its body",
                why: "A generic method template needs a body at collection; a separate overload signature has no template body.",
                collision: "compiler.md §82.4",
            };

pub(super) const DISPOSEASYNC: DivergenceEntry = DivergenceEntry {
    ts: "class A { async [Symbol.dispose]():Promise<void> {} }\nexport function main(): void {  }",
    subscript: "no equivalent; declare a synchronous disposal hook",
    why: "A disposal hook must run synchronously.",
    collision: "compiler.md §60.1",
};

pub(super) const DISPOSESIGNATURE: DivergenceEntry = DivergenceEntry {
    ts: "class A { [Symbol.dispose](v:i32):void {} }\nexport function main(): void {  }",
    subscript: "no equivalent; declare Symbol.dispose with no parameter and a void return",
    why: "A disposal hook takes no parameter and returns void.",
    collision: "compiler.md §60.1",
};

pub(super) const DESCRIPTORINHERITANCE: DivergenceEntry = DivergenceEntry {
    ts: "class Base {}\n@Descriptor\nclass A extends Base {}\nexport function main(): void {  }",
    subscript: "no equivalent; declare all descriptor fields directly",
    why: "A descriptor class declares its own data and does not inherit.",
    collision: "compiler.md §25.1",
};

pub(super) const REFERENCECLASSINHERITANCE: DivergenceEntry = DivergenceEntry {
                ts: "class Base {}\n\nclass A extends Base { constructor() { super(); } }\nexport function main(): void {  }",
                subscript: "no equivalent; use composition through a class field",
                why: "A reference class has its own nominal identity and C layout; the language has no class inheritance.",
                collision: "compiler.md §115.1",
            };

pub(super) const DESCRIPTORSTATICFIELD: DivergenceEntry = DivergenceEntry {
    ts: "@Descriptor\nclass A { static x:i32=1; }\nexport function main(): void {  }",
    subscript: "no equivalent; move the static data to a module binding",
    why: "A descriptor class declares instance data only and has no static field.",
    collision: "compiler.md §71.1",
};

pub(super) const MIRRORSTATICFIELD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare class A_class_shape_493 { static x:i32; }",
                subscript: "no equivalent; access host data through a foreign function",
                why: "A mirror class describes C instance storage and has no static field.",
                collision: "compiler.md §71.1",
            };

pub(super) const STATICFIELDOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "class A { static x?:i32; }\nexport function main(): void {  }",
                subscript: "no equivalent; use an initialized nullable field",
                why: "A static field cannot represent undefined; optional fields belong only to the descriptor surface.",
                collision: "C7",
            };

pub(super) const CONTEXTAFFINESTATICFIELD: DivergenceEntry = DivergenceEntry {
                ts: "class A { static x:Inbox<A>; }\nexport function main(): void {  }",
                subscript: "no equivalent; keep the handle in a local binding",
                why: "Worker, Inbox, and Outbox values belong to one Context and cannot enter static field storage.",
                collision: "compiler.md §40",
            };

pub(super) const DESCRIPTORINITIALIZERWITHOUTOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { x:i32=1; }\nexport function main(): void {  }",
                subscript: "no equivalent; spell a defaulted member as name?: T = value",
                why: "A descriptor member with a default initializer must use the optional question-mark spelling.",
                collision: "compiler.md §25.1",
            };

pub(super) const DESCRIPTORREQUIREDWITHOUTDEFINITE: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { x:i32;constructor(){this.x=1;} }\nexport function main(): void {  }",
                subscript: "no equivalent; spell a required member as name!: T",
                why: "A required descriptor member must use the definite-assignment spelling so a constructing literal supplies its value.",
                collision: "compiler.md §25.1",
            };

pub(super) const INSTANCEFIELDOPTIONAL: DivergenceEntry = DivergenceEntry {
                ts: "class A { x?:i32; }\nexport function main(): void {  }",
                subscript: "no equivalent; use an initialized nullable field",
                why: "An ordinary instance field cannot represent undefined; optional fields belong only to the descriptor surface.",
                collision: "C7",
            };

pub(super) const WIREALIASNESTEDFIELD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_class_shape_624=CEnum<{a:1}>; declare class A_class_shape_624 { x:FixedArray<W_class_shape_624,2>; }",
                subscript: "no equivalent; use a direct wire-alias field or an array-pair element",
                why: "A wire alias occupies a direct boundary field or an array-pair element; a nested boundary type has no declared wire position.",
                collision: "compiler.md §52.2",
            };

pub(super) const CONTEXTAFFINEINSTANCEFIELD: DivergenceEntry = DivergenceEntry {
                ts: "class A { x!:Inbox<A>; }\nexport function main(): void {  }",
                subscript: "no equivalent; keep the handle in a local binding",
                why: "Worker, Inbox, and Outbox values belong to one Context and cannot enter instance field storage.",
                collision: "compiler.md §40",
            };

pub(super) const VALUEFIELDOUTSIDEWHITELIST: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType\nclass A { x:string=\"x\"; }\nexport function main(): void {  }",
                subscript: "no equivalent; store the value in a reference class",
                why: "A value class contains only sized numerics, booleans, value classes, fixed arrays, enums, and admitted literal aliases.",
                collision: "C2",
            };

pub(super) const DESCRIPTORCONSTRUCTOR: DivergenceEntry = DivergenceEntry {
                ts: "@Descriptor\nclass A { constructor() {} }\nexport function main(): void {  }",
                subscript: "no equivalent; construct the descriptor with an object literal",
                why: "A descriptor class receives its fields from a constructing literal and declares no constructor.",
                collision: "compiler.md §25.1",
            };

pub(super) const WIREALIASNESTEDCONSTRUCTORPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_class_shape_719=CEnum<{a:1}>; declare class A_class_shape_719 { constructor(x:FixedArray<W_class_shape_719,2>); }",
                subscript: "no equivalent; use a direct wire-alias constructor parameter",
                why: "A wire alias is a direct mirror constructor parameter or an array-pair element; other nested positions have no wire representation.",
                collision: "compiler.md §52.2",
            };

pub(super) const CLASSINDEXSIGNATURECOUNT: DivergenceEntry = DivergenceEntry {
    ts: "class A { [a:i32]:i32; [b:string]:i32; }\nexport function main(): void {  }",
    subscript: "no equivalent; declare one class index signature",
    why: "A class declares at most one index signature for its accessor pair.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATURENONREFERENCE: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType\nclass A { [a:i32]:i32; }\nexport function main(): void {  }",
    subscript: "no equivalent; declare the index signature on an ordinary reference class",
    why: "Only an ordinary reference class can declare an index signature.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATURESTATIC: DivergenceEntry = DivergenceEntry {
    ts: "class A { static [a:i32]:i32; }\nexport function main(): void {  }",
    subscript: "no equivalent; declare an instance index signature",
    why: "A class index signature describes instance access through its get and set methods.",
    collision: "compiler.md §58.1",
};

pub(super) const CLASSINDEXSIGNATUREINDEXTYPE: DivergenceEntry = DivergenceEntry {
    ts: "class A { [a:string]:i32; }\nexport function main(): void {  }",
    subscript: "no equivalent; use an i32 or u32 index",
    why: "A class index signature requires an i32 or u32 index.",
    collision: "compiler.md §58.1",
};

pub(super) const WRITEACCESSORWITHOUTREAD: DivergenceEntry = DivergenceEntry {
                ts: "class C { set x(v: i32) {} static set y(v: i32) {} }\nexport function main(): void { const c = new C(); c.x; c.x = 1; C.y; C.y = 1; }",
                subscript: "no equivalent; declare a read accessor with the same name",
                why: "A write accessor needs a read accessor because property writes use the shared accessor type.",
                collision: "compiler.md §65.1",
            };

pub(super) const ACCESSORTYPEMISMATCH: DivergenceEntry = DivergenceEntry {
    ts: "class A { get x():i32 {return 1;} set x(v:string) {} }\nexport function main(): void {  }",
    subscript: "no equivalent; use the same type for both accessors",
    why: "A read accessor and its write accessor must share exactly one type.",
    collision: "compiler.md §65.1",
};

pub(super) const CLASSINDEXSETSIGNATURE: DivergenceEntry = DivergenceEntry {
                ts: "class A { [a:i32]:i32; }\nexport function main(): void {  }",
                subscript: "no equivalent; declare set(index: I, value: T): void with matching types",
                why: "A mutable class index signature needs a synchronous set method with exactly matching index and element types.",
                collision: "compiler.md §58.1",
            };

pub(super) const MODULEUSING: DivergenceEntry = DivergenceEntry {
                ts: "using x = { [Symbol.dispose]():void {} };\nexport function main(): void {  }",
                subscript: "no equivalent; move the using binding into a function block",
                why: "A using binding needs a function block scope for deterministic disposal; module bindings have no such exit.",
                collision: "compiler.md §60.1",
            };

pub(super) const DESCRIPTOROPTIONS: DivergenceEntry = DivergenceEntry {
                ts: "function Descriptor(options:object):(target:object,context:ClassDecoratorContext)=>void {return (target:object,context:ClassDecoratorContext):void=>{};}\n@Descriptor({}) class A {}\nexport function main(): void {  }",
                subscript: "no equivalent; use the bare Descriptor decorator",
                why: "The Descriptor decorator accepts no options.",
                collision: "compiler.md §62.1",
            };

pub(super) const UNSUPPORTEDCLASSDECORATOR: DivergenceEntry = DivergenceEntry {
                ts: "function other(value:object,context:ClassDecoratorContext):void {}\n@other\nclass A {}\nexport function main(): void {  }",
                subscript: "no equivalent; use an ambient ValueType or Descriptor decorator",
                why: "Only the ambient ValueType and Descriptor decorators define a class representation.",
                collision: "compiler.md §130.1",
            };

pub(super) const DESCRIPTORVALUETYPE: DivergenceEntry = DivergenceEntry {
    ts: "@ValueType\n@Descriptor\nclass A {}\nexport function main(): void {  }",
    subscript: "no equivalent; use the Descriptor decorator alone",
    why: "A descriptor is a reference class and cannot also declare value-class representation.",
    collision: "compiler.md §25.1",
};

pub(super) const ENUMIMPLICITVALUEOVERFLOW: DivergenceEntry = DivergenceEntry {
    ts: "enum E { a=2147483647,b }\nexport function main(): void {  }",
    subscript: "no equivalent; give the member an explicit value in the i32 range",
    why: "An enum member occupies i32 storage, so an implicit next value must fit the i32 range.",
    collision: "compiler.md §72",
};

pub(super) const WIREENUMEMPTY: DivergenceEntry = DivergenceEntry {
    ts: "type A = CEnum<{}>;\nexport function main(): void {  }",
    subscript: "no equivalent; declare at least one wire member",
    why: "A CEnum wire mapping must contain at least one member.",
    collision: "compiler.md §50.1",
};

pub(super) const WIREENUMMEMBERFORM: DivergenceEntry = DivergenceEntry {
                ts: "type A=CEnum<{[key:string]:number}>;\nexport function main(): void {  }",
                subscript: "no equivalent; declare named properties with integer-literal types",
                why: "A CEnum mapping consists of named properties whose types are integer literals, rather than methods or index signatures.",
                collision: "compiler.md §50.1",
            };

pub(super) const WIREENUMMEMBERKEY: DivergenceEntry = DivergenceEntry {
                ts: "type A = CEnum<{ 1:1 }>;\nexport function main(): void {  }",
                subscript: "no equivalent; use an identifier or string literal as the member key",
                why: "A CEnum member key must name a string-literal union member, so numeric property keys are excluded.",
                collision: "compiler.md §50.1",
            };

pub(super) const MIRRORVARIABLEFORM: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare const A_declarations_818:i32;",
                subscript: "no equivalent; declare a const with a non-negative integer-literal initializer",
                why: "A mirror binds constants with non-negative integer-literal values; it does not bind a host data symbol.",
                collision: "compiler.md §136.1",
            };

pub(super) const WIREALIASNESTEDFOREIGNPARAMETER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_signatures_47=CEnum<{a:1}>;declare function f_signatures_47(x:FixedArray<W_signatures_47,2>):void;",
                subscript: "no equivalent; use a direct wire-alias parameter",
                why: "A wire alias is a direct foreign parameter or an array-pair element; another nested position has no declared wire representation.",
                collision: "compiler.md §52.2",
            };

pub(super) const WIREALIASNESTEDFOREIGNRETURN: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ntype W_signatures_60=CEnum<{a:1}>;declare function f_signatures_60():W_signatures_60[];",
                subscript: "no equivalent; return a direct wire alias",
                why: "A wire alias has a direct foreign return representation only; an array return has no declared wire representation.",
                collision: "compiler.md §50.2",
            };

pub(super) const FOREIGNDIRECTCALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare function f_signatures_79(cb:(x:i32)=>void):void;",
                subscript: "no equivalent; put the callback in a mirrored boundary struct",
                why: "A callback occupies a mirrored boundary-struct field; a direct foreign callback parameter has no declared provenance position.",
                collision: "compiler.md §23.3",
            };

pub(super) const FOREIGNRETURNPROVENANCE: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\ndeclare function f_signatures_106():string;",
                subscript: "no equivalent; return data through a mirrored boundary struct",
                why: "A foreign string-view, descriptor, or callback return has no return provenance in the boundary vocabulary.",
                collision: "compiler.md §23.3",
            };

pub(super) const FOREIGNRESULTREAD: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nexport function main(): void {  }\n// file: mirror.d.ts\n// @subscript-c-header include=\"result-read.h\"\ndeclare class L { tag: i32; items: u32[]; constructor(tag: i32, items: u32[]); }\ndeclare class P { x: i32; constructor(x: i32); }\ndeclare class H { k: i32; p: P | null; constructor(k: i32, p: P | null); }\ndeclare class U { name: string; ud: object | null; constructor(name: string, ud: object | null); }\ndeclare class S { name: string; l: L; constructor(name: string, l: L); }\ntype OnL = (l: L) => void;\ndeclare class C { cb: OnL; ud: object | null; constructor(cb: OnL, ud: object | null); }\ndeclare function f_result_read(): L;\ndeclare function f_fill_pointer(out: H | null): void;\ndeclare function f_fill_userdata(out: U | null): void;\ndeclare function f_fill_nested(out: S | null): void;\ndeclare function f_fill_elements(items: L[]): void;\ndeclare function f_register(c: C): void;",
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
                subscript: "no equivalent; use an async function or a synchronous generator",
                why: "An async function uses a Promise<T> return view; an async generator needs an async iterator return view.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNANNOTATIONMISSING: DivergenceEntry = DivergenceEntry {
                ts: "async function f() {}\nexport function main(): void {  }",
                subscript: "no equivalent; annotate the return with Promise<T>",
                why: "An async function must declare its suspendable return view with an explicit Promise<T> annotation.",
                collision: "compiler.md §26.1",
            };

pub(super) const OPTIONALPARAMETER: DivergenceEntry = DivergenceEntry {
    ts: "function f(x?:i32):void {}\nfunction apply(cb:(x?:i32)=>void):void { cb(); }\nexport function main(): void {  }",
    subscript: "no equivalent; use a default parameter or an explicitly nullable parameter",
    why: "An optional parameter implies undefined, which has no language value.",
    collision: "C7",
};

pub(super) const ERRORMESSAGETYPE: DivergenceEntry = DivergenceEntry {
                ts: "type Message=\"a\"|\"b\";\nexport function main(): void { const message:Message=\"a\";const e=new Error(message); }",
                subscript: "no equivalent; format the message as a template literal",
                why: "An Error message must be a string; a nominal literal alias needs explicit formatting.",
                collision: "compiler.md §115.1",
            };

pub(super) const ERRORCONSTRUCTORARGUMENTS: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { const e=new Error(\"x\",{cause:1}); }",
                subscript: "no equivalent; pass zero arguments or one string message",
                why: "An Error constructor accepts an optional string message only; options and spread arguments are outside its surface.",
                collision: "compiler.md §115.1",
            };

pub(super) const ERRORCALLWITHOUTNEW: DivergenceEntry = DivergenceEntry {
    ts: "export function main(): void { const e=Error(\"x\"); }",
    subscript: "no equivalent; construct the Error with new",
    why: "An Error-family object must use the new constructor spelling.",
    collision: "compiler.md §115.1",
};

pub(super) const VALUETYPEARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType() class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "The ValueType decorator requires exactly one object-literal argument without a spread.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONSNONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType(1) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "The ValueType decorator requires an object-literal options argument.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONCOUNT: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType options literal must contain exactly the align property.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONSPREAD: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({...{align:2}}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType options literal declares align directly and does not spread properties.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONPROPERTYFORM: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({align(){}}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType options literal declares align as a key-value property.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEOPTIONKEY: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({other:2}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType options literal accepts only the align key.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEALIGNMENTNONLITERAL: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({align:\"2\"}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType alignment must be a numeric integer literal.",
                collision: "compiler.md §62.1",
            };

pub(super) const VALUETYPEALIGNMENTOUTSIDESET: DivergenceEntry = DivergenceEntry {
                ts: "function ValueType(...args:unknown[]):(target:object, context:ClassDecoratorContext)=>void{return (target:object,context:ClassDecoratorContext):void=>{};}\n@ValueType({align:3}) class A{}\nexport function main():void{}",
                subscript: "no equivalent; use ValueType({align: 16}) with a literal alignment",
                why: "A ValueType alignment must be one of 2, 4, 8, or 16.",
                collision: "compiler.md §62.1",
            };

pub(super) const INVALIDPROGRAMENTRY: DivergenceEntry = DivergenceEntry {
                ts: "// file: a.ts\nexport function read(): void {}\n// file: b.ts\nexport function write(): void {}",
                subscript: "no equivalent; name one non-ambient entry module in the build input",
                why: "A program with multiple source files must name exactly one non-ambient entry module in its build input.",
                collision: "compiler.md §129.1",
            };

pub(super) const ASYNCRETURNNONREFERENCE: DivergenceEntry = DivergenceEntry {
    ts: "async function f(): (Promise<void>) {}\nexport function main(): void {}",
    subscript: "no equivalent; annotate the return with Promise<T>",
    why: "An async function must spell its suspendable return view as Promise<T>.",
    collision: "compiler.md §26.1",
};

pub(super) const ASYNCRETURNQUALIFIEDNAME: DivergenceEntry = DivergenceEntry {
                ts: "async function f(): globalThis.Promise<void> {}\nexport function main(): void {}",
                subscript: "no equivalent; annotate the return with the unqualified Promise<T> name",
                why: "An async function must spell its suspendable return view with the unqualified Promise<T> name.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNALIAS: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"alias.h\"\ntype P = Promise<i32>;\n// file: main.ts\nexport async function f(): P { return 1; }\n",
                subscript: "no equivalent; annotate the return with Promise<T> instead of an alias",
                why: "An async function must spell its suspendable return view as Promise<T>, rather than an alias.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNMISSINGARGUMENT: DivergenceEntry = DivergenceEntry {
                ts: "type Promise<T=void>=globalThis.Promise<T>; async function f():Promise {}\nexport function main(): void {}",
                subscript: "no equivalent; supply the fulfilled-value type in Promise<T>",
                why: "The suspendable return view must state exactly one fulfilled-value type.",
                collision: "compiler.md §26.1",
            };

pub(super) const ASYNCRETURNARGUMENTCOUNT: DivergenceEntry = DivergenceEntry {
                ts: "type Promise<T,U>=globalThis.Promise<T>; async function f():Promise<void,i32> {}\nexport function main(): void {}",
                subscript: "no equivalent; supply exactly one fulfilled-value type in Promise<T>",
                why: "The suspendable return view must state exactly one fulfilled-value type.",
                collision: "compiler.md §26.1",
            };

pub(super) const FIXEDARRAYLENGTHRANGE: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: FixedArray<u8, 4294967296>): void {}\nexport function main(): void {}",
                subscript: "no equivalent; use a smaller FixedArray length or a dynamic array",
                why: "A FixedArray must fit the signed aggregate byte limit, which excludes lengths above the element-count range.",
                collision: "collisions.md Q29",
            };

pub(super) const FIXEDARRAYLENGTHLITERAL: DivergenceEntry = DivergenceEntry {
    ts: "function f(x: FixedArray<u8, -1>): void {}\nexport function main(): void {}",
    subscript: "no equivalent; supply a non-negative integer literal as the FixedArray length",
    why: "A C array layout needs a non-negative integer literal length at compile time.",
    collision: "collisions.md Q3",
};

pub(super) const GENERICCONSTRAINTIDENTITY: DivergenceEntry = DivergenceEntry {
                ts: "class C { x: i32 = 1; } class D { x: i32 = 1; } function f<T extends C>(x:T): void {} export function main(): void { f<D>(new D()); }",
                subscript: "no equivalent; pass an instance of the declared constraint class",
                why: "A generic constraint uses nominal class identity, rather than TypeScript structural compatibility.",
                collision: "C1",
            };

pub(super) const NAMESPACEUNEXPORTEDMEMBER: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as ns from \"./namespace-default-export-lib\"; export function main():void {ns.default();}\n// file: namespace-default-export-lib.ts\nexport default function f():void {}",
                subscript: "no equivalent; use a declared named export",
                why: "A namespace qualifier exposes only the declared named exports; a default export stays outside the module surface.",
                collision: "C18",
            };

pub(super) const RUNNERMAINMISSING: DivergenceEntry = DivergenceEntry {
                ts: "export function update(): void {}",
                subscript: "no equivalent; export main from the entry module",
                why: "The corpus runner calls the entry module host function main; a library export cannot supply that entry.",
                collision: "collisions.md Q12",
            };

pub(super) const CLASSFINALALIGNMENTLIMIT: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType({align:16}) class C { x:FixedArray<u8,2147483647>; constructor(x:FixedArray<u8,2147483647>){this.x=x;} }\nexport function main(): void {}",
                subscript: "no equivalent; reduce the class fields or their declared alignment",
                why: "Final alignment contributes bytes to the object layout and must stay within the signed displacement limit.",
                collision: "collisions.md Q29",
            };

pub(super) const AGGREGATEARGUMENTFRAMELIMIT: DivergenceEntry = DivergenceEntry {
                ts: "function consume(a:FixedArray<u8,1100000000>,b:FixedArray<u8,1100000000>):void{} function probe(a:FixedArray<u8,1100000000>,b:FixedArray<u8,1100000000>):void{consume(a,b);} export function main():void{}",
                subscript: "no equivalent; pass a reference class instead of a large value aggregate",
                why: "Argument copies occupy stack storage and must stay within the accumulated frame limit.",
                collision: "collisions.md Q29",
            };

pub(super) const NAMESPACEASVALUE: DivergenceEntry = DivergenceEntry {
                ts: "// file: main.ts\nimport * as ns from \"./namespace-local-function-lib\"; export function main(): void { const typed: object = ns; } export function probe(): void { function ns(): void {} ns(); }\n// file: namespace-local-function-lib.ts\nexport function f(): void {}",
                subscript: "no equivalent; use the namespace only as a qualifier of a named export",
                why: "A namespace import is a static qualifier and has no runtime value.",
                collision: "C18",
            };

pub(super) const SWITCHCASEREAD: DivergenceEntry = DivergenceEntry {
                ts: "export function main():void {const n:i32=1; switch(n){case 0:let x:i32=1;break;case 1:const f:()=>i32=():i32=>x;print(`${f()}`);break;}}",
                subscript: "no equivalent; declare the shared local before the switch",
                why: "A closure can run before the initializer of a binding in another case.",
                collision: "C14",
            };

pub(super) const BLOCKNAMEREADBEFOREDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "class C {} export function main(): void { const f:()=>i32=():i32=>C; const C:i32=1; }",
                subscript: "no equivalent; declare the local before the closure that reads it",
                why: "A block owns each declared name throughout its scope; an earlier read must not resolve to an outer declaration.",
                collision: "C14",
            };

pub(super) const BLOCKNAMEWRITEBEFOREDECLARATION: DivergenceEntry = DivergenceEntry {
                ts: "export function main():void { const f:()=>void=():void=>{x=2;}; let x:i32=1; f(); }",
                subscript: "no equivalent; declare the local before the closure that writes it",
                why: "A block owns each declared name throughout its scope; an earlier write must not target an outer declaration.",
                collision: "C14",
            };

pub(super) const CONTEXTAFFINECAPTURE: DivergenceEntry = DivergenceEntry {
                ts: "class M {} function f(box: Inbox<M>): void { const read:()=>void=():void=>{print(`${box}`);}; read(); }\nexport function main(): void {}",
                subscript: "no equivalent; keep the worker handle in the function that owns it",
                why: "A worker handle belongs to one Context and cannot enter a closure environment.",
                collision: "compiler.md §40",
            };

pub(super) const MUTABLELOCALCAPTURE: DivergenceEntry = DivergenceEntry {
                ts: "export function main(): void { let x:i32=1; const f:()=>i32=():i32=>x; print(`${f()}`); }",
                subscript: "no equivalent; capture a const local by value",
                why: "A closure copies const local values; mutable captures need shared storage that the language does not provide.",
                collision: "C5",
            };

pub(super) const CONTEXTAFFINEARRAYELEMENT: DivergenceEntry = DivergenceEntry {
    ts: "class M {} function f(x:Inbox<M>[]):void {}\nexport function main(): void {}",
    subscript: "no equivalent; keep worker handles in local variables",
    why: "A worker handle belongs to one Context and cannot enter array storage.",
    collision: "compiler.md §40",
};

pub(super) const CONTEXTAFFINECONTAINERARGUMENT: DivergenceEntry = DivergenceEntry {
    ts: "class M {} function f(x:Map<i32,Inbox<M>>):void {}\nexport function main(): void {}",
    subscript: "no equivalent; keep worker handles in local variables",
    why: "A worker handle belongs to one Context and cannot enter container storage.",
    collision: "compiler.md §40",
};

pub(super) const WORKERMESSAGEPLAINCLASS: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class M { x: i32 = 1; } function f(x: Worker<M, M>): void {}",
                subscript: "no equivalent; declare a plain reference class for the message",
                why: "Worker messaging copies a plain reference class; scalar and decorated message types have no declared transfer shape.",
                collision: "stdlib.md §16.2",
            };

pub(super) const BOUNDARYLITERALALIAS: DivergenceEntry = DivergenceEntry {
                ts: "// file: boundary-alias.ts\nexport function main(): void {}\n// file: boundary-alias.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare type Choice=\"a\"|\"b\";\ndeclare function foreign(x:Choice):void;",
                subscript: "no equivalent; declare a CEnum alias for the C wire value",
                why: "A string-literal alias has no C wire representation for a mirror signature.",
                collision: "compiler.md §24.1",
            };

pub(super) const NULLABLENONREFERENCE: DivergenceEntry = DivergenceEntry {
                ts: "function f(x: i32 | null): void {}\nexport function main(): void {}",
                subscript: "no equivalent; use a reference class to represent a nullable value",
                why: "Only reference classes, opaque handles, function types, and boundary pointers have a nullable representation.",
                collision: "C7",
            };

pub(super) const NULLABLEVALUECLASSASSIGNMENT: DivergenceEntry = DivergenceEntry {
                ts: "@ValueType class C {} interface C {():i32;} function f(c:C):void {const fn:(()=>i32)|null=c;}\nexport function main(): void {}",
                subscript: "no equivalent; use a reference class to represent a nullable value",
                why: "A value class stores its fields inline and has no nullable pointer representation.",
                collision: "C7",
            };

pub(super) const MIRRORHEADERMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-header.ts\nexport function main(): void {}\n// file: prov-header.d.ts\ndeclare function foreign(x:i32):void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "Foreign functions need a header identity for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORPARAMETERTARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-parameter.ts\nexport function main(): void {}\n// file: prov-target-parameter.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-string-view function=\"missing\" parameter=\"x\" aggregate=\"View\"\ndeclare function foreign():void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "Parameter provenance must name a parameter declared in the same mirror.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORCALLBACKTARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-callback.ts\nexport function main(): void {}\n// file: prov-target-callback.d.ts\n// @subscript-c-callback typedef=\"Missing\"",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "Callback provenance must name a callback typedef declared in the same mirror.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORLIFETIMETARGETMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-target-lifetime.ts\nexport function main(): void {}\n// file: prov-target-lifetime.d.ts\n// @subscript-c-callback-lifetime aggregate=\"Missing\"",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A callback lifetime record must name a boundary class that carries a callback field.",
                collision: "compiler.md §111",
            };

pub(super) const MIRRORARRAYPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-array.ts\nexport function main(): void {}\n// file: prov-array.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign(xs:i32[]):void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "An absorbed array parameter needs its C aggregate or scalar-pair provenance for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORSTRINGPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-string.ts\nexport function main(): void {}\n// file: prov-string.d.ts\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign(x:string):void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "An absorbed string parameter needs its C string-view aggregate identity for C emission.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORPARAMETERPROVENANCEMISMATCH: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-incompatible.ts\nexport function main(): void {}\n// file: prov-incompatible.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-string-view function=\"foreign\" parameter=\"x\" aggregate=\"View\"\ndeclare function foreign(x:i32):void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "The declared parameter type must agree with its recorded C parameter shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORANONYMOUSCALLBACK: DivergenceEntry = DivergenceEntry {
                ts: "// file: callback-anonymous-field.ts\nexport function main():void {}\n// file: callback-anonymous-field.d.ts\ndeclare class C {cb:(x:i32)=>void;}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A callback needs a named C typedef for its trampoline cast.",
                collision: "compiler.md §23.3",
            };

pub(super) const MIRRORCALLBACKPROVENANCEMISSING: DivergenceEntry = DivergenceEntry {
                ts: "// file: callback-alias-field.ts\nexport function main():void {}\n// file: callback-alias-field.d.ts\ndeclare type Callback=(x:i32)=>void; declare class C {cb:Callback;}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A callback needs its recorded C typedef identity for its trampoline cast.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNKNOWNKIND: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-unknown\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance record must use a declared record kind.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEMISSINGKIND: DivergenceEntry = DivergenceEntry {
    ts: "// file: mirror.d.ts\n// @subscript-c-\n// file: main.ts\nexport function main():void{}",
    subscript: "no equivalent; regenerate the mirror from its C header",
    why: "A provenance record needs a record kind.",
    collision: "compiler.md §23.3",
};

pub(super) const PROVENANCEFIELDSEPARATOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\"parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "Whitespace must separate provenance fields.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNEXPECTEDKEY: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header wrong=\"a.h\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "Each provenance field must use the key required by its record kind.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNQUOTEDSTRING: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=a.h\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance string must use quotes.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNTERMINATEDSTRING: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance string must have a closing quote.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNTERMINATEDESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\\\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance escape must contain its escaped character.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEUNSUPPORTEDESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\z\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance string must use a declared escape form.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCECONTROLCHARACTER: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a\x01b\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance string must escape control characters.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDUNICODEDIGITS: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\uGGGG\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A Unicode escape must contain four hexadecimal digits.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDBOOLEAN: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=other\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance boolean must use true or false.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCETRAILINGDATA: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"a.h\" extra\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance record must end after its declared fields.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCESHORTUNICODEESCAPE: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\u12\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A Unicode escape must contain four hexadecimal digits.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEINVALIDUNICODESCALAR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-header include=\"\\uD800\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A provenance string must contain Unicode scalar values.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEHEADERBASENAME: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-malformed.ts\nexport function main(): void {}\n// file: prov-malformed.d.ts\n// @subscript-c-header include=\"bad/path.h\"\ndeclare function foreign():void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A header identity must be a nonempty basename without control characters.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEHEADER: DivergenceEntry = DivergenceEntry {
                ts: "// file: prov-duplicate.ts\nexport function main(): void {}\n// file: prov-duplicate.d.ts\n// @subscript-c-header include=\"x.h\"\n// @subscript-c-header include=\"x.h\"\ndeclare function foreign():void;",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A mirror has one header identity.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYDESCRIPTOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A descriptor record needs nonempty function, parameter, aggregate, and element identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATEDESCRIPTOR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// @subscript-c-descriptor function=\"f\" parameter=\"p\" aggregate=\"A\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYSTRINGVIEW: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-string-view function=\"\" parameter=\"p\" aggregate=\"A\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A string-view record needs nonempty function, parameter, and aggregate identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATESTRINGVIEW: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-string-view function=\"f\" parameter=\"p\" aggregate=\"A\"\n// @subscript-c-string-view function=\"f\" parameter=\"p\" aggregate=\"A\"\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEEMPTYSCALARPAIR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-scalar-pair function=\"\" parameter=\"p\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "A scalar-pair record needs nonempty function, parameter, and element identities.",
                collision: "compiler.md §23.3",
            };

pub(super) const PROVENANCEDUPLICATESCALARPAIR: DivergenceEntry = DivergenceEntry {
                ts: "// file: mirror.d.ts\n// @subscript-c-scalar-pair function=\"f\" parameter=\"p\" element=\"E\" const=true\n// @subscript-c-scalar-pair function=\"f\" parameter=\"p\" element=\"E\" const=true\n// file: main.ts\nexport function main():void{}",
                subscript: "no equivalent; regenerate the mirror from its C header",
                why: "One parameter has one recorded C shape.",
                collision: "compiler.md §23.3",
            };
