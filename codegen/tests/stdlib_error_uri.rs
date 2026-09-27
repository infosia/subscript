use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn check_engines(source: &str, expected: &[u8]) {
    let files = [SourceFile::new("test.ts", source)];
    let module = check_program(&files).unwrap();
    let lir = lower_module(&module).unwrap();
    assert_eq!(interpret(&lir).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn error_classes_and_uri_match_the_golden() {
    check_engines(
        include_str!("../../corpus/accept/a267-error-classes-and-uri.ts"),
        include_bytes!("../../corpus/accept/a267-error-classes-and-uri.expected"),
    );
}

#[test]
fn decode_raises_through_calls_and_receiver_evaluates_once() {
    check_engines(r#"
        function decode(s: string): string { return decodeURI(s); }
        function error(): Error { print("once"); return new RangeError("m"); }
        export function main(): void {
            print(error().toString());
            try { print(decode("é%FF")); } catch (e) {
                if (e instanceof URIError) { print(e.message); }
            }
            const indirect: (s: string) => string = (s: string): string => decodeURIComponent(s);
            try { print(indirect("é%20%ED%A0%80")); } catch (e) {
                if (e instanceof URIError) { print(e.message); }
            }
            print(decode("a%20b"));
        }
    "#, b"once\nRangeError: m\ndecodeURI: malformed escape at byte 2\ndecodeURIComponent: malformed escape at byte 5\na b\n");
}

#[test]
fn every_error_kind_formats_and_uses_its_tag_after_name_mutation() {
    let names = [
        "Error",
        "SyntaxError",
        "TypeError",
        "RangeError",
        "ReferenceError",
        "EvalError",
        "URIError",
    ];
    let mut source = String::from("export function main(): void {\n");
    let mut expected = String::new();
    for (index, name) in names.iter().enumerate() {
        source.push_str(&format!("const e{index}: Error = new {name}(); e{index}.name = \"\"; e{index}.message = \"x\"; print(e{index}.toString()); e{index}.name = \"N\"; e{index}.message = \"\"; print(e{index}.toString());\n"));
        expected.push_str("x\nN\n");
        for (other_index, other) in names.iter().enumerate() {
            source.push_str(&format!("print(`${{e{index} instanceof {other}}}`);\n"));
            expected.push_str(if other_index == 0 || other_index == index {
                "true\n"
            } else {
                "false\n"
            });
        }
    }
    source.push_str("}\n");
    check_engines(&source, expected.as_bytes());
}

#[test]
fn uri_coercions_and_error_json_are_rejected() {
    for function in [
        "encodeURI",
        "encodeURIComponent",
        "decodeURI",
        "decodeURIComponent",
    ] {
        for arg in ["1", "true"] {
            let source = format!("export function main(): void {{ {function}({arg}); }}");
            assert!(check_program(&[SourceFile::new("test.ts", &source)]).is_err());
        }
        let source = format!("export function main(): void {{ {function}(\"x\"); }}");
        assert!(check_program(&[SourceFile::new("test.ts", &source)]).is_ok());
    }
    for name in [
        "Error",
        "SyntaxError",
        "TypeError",
        "RangeError",
        "ReferenceError",
        "EvalError",
        "URIError",
    ] {
        let source =
            format!("export function main(): void {{ JSON.stringify(new {name}(\"m\")); }}");
        assert!(check_program(&[SourceFile::new("test.ts", &source)]).is_err());
    }
    assert!(check_program(&[SourceFile::new(
        "test.ts",
        "export function main(): void { JSON.stringify(1); }"
    )])
    .is_ok());
}
