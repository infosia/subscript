use subscript_compiler::SourceFile;

pub fn program(iterations: usize, immutable: bool) -> Vec<SourceFile> {
    let calls = if immutable {
        "const g = gen();
        try { gx(g); } catch (e) { caught++; }
        try { gy(g, {}); } catch (e) { caught++; }"
    } else {
        "try { gx(gen()); } catch (e) { caught++; }
        try { gy(gen(), {}); } catch (e) { caught++; }"
    };
    vec![SourceFile::new(
        "operand-raise.ts",
        format!(
            r#"
function boom(): i32 {{ throw new Error("boom"); }}
let fire: boolean = false;
function initX(): i32 {{ if (fire) {{ return boom(); }} return 1; }}
@Descriptor
class D {{ x?: i32 = initX(); }}
function* gen(): Generator<i32> {{ yield 1; }}
function gx(g: Generator<i32>, d: D = {{}}): i32 {{ return d.x; }}
function gy(g: Generator<i32>, d: D): i32 {{ return d.x; }}
export function main(): void {{
    print(`${{gx(gen())}}`);
    fire = true;
    let caught: i32 = 0;
    for (let i = 0; i < {iterations}; i++) {{ {calls} }}
    print(`caught ${{caught}}`);
}}
"#
        ),
    )]
}

pub fn new_program(iterations: usize, immutable: bool) -> Vec<SourceFile> {
    let call = if immutable {
        "const g = gen(); try { new C(g); } catch (e) { caught++; }"
    } else {
        "try { new C(gen()); } catch (e) { caught++; }"
    };
    vec![SourceFile::new(
        "new-operand-raise.ts",
        format!(
            r#"
function boom(): i32 {{ throw new Error("boom"); }}
let fire: boolean = false;
function initX(): i32 {{ if (fire) {{ return boom(); }} return 1; }}
class C {{ x: i32 = initX(); constructor(g: Generator<i32>) {{}} }}
function* gen(): Generator<i32> {{ yield 1; }}
export function main(): void {{
    fire = true;
    let caught: i32 = 0;
    for (let i = 0; i < {iterations}; i++) {{ {call} }}
    print(`caught ${{caught}}`);
}}
"#
        ),
    )]
}
