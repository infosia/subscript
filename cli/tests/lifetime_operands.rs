//! Operand lifetime checks through the CLI (compiler.md §120).

use std::{fs, process::Command};

fn probe(name: &str, setup: &str, operation: &str, column: u32) {
    let directory =
        std::env::temp_dir().join(format!("subscript-lifetime-{}-{name}", std::process::id()));
    fs::create_dir_all(&directory).expect("probe directory");
    for freed in [false, true] {
        let name = format!("{name}-{freed}");
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("export function main(): void {{\n  print(\"before\");\n  {setup}\n  {release}\n  {operation}\n  print(\"done\");\n}}\n");
        let path = directory.join(format!("{name}.ts"));
        fs::write(&path, source).expect("probe source");
        let actual = Command::new(env!("CARGO_BIN_EXE_subscript"))
            .arg("run")
            .arg(&path)
            .output()
            .expect("CLI dev JIT");
        if freed {
            assert!(!actual.status.success(), "{name}: {actual:?}");
            assert_eq!(actual.stdout, b"before\n", "{name}");
            let stderr = String::from_utf8(actual.stderr).unwrap();
            assert!(
                stderr.contains(&format!(":5:{column}: trap [use-after-delete]:")),
                "{name}: {stderr}"
            );
        } else {
            assert!(actual.status.success(), "{name}: {actual:?}");
            assert_eq!(actual.stdout, b"before\ndone\n", "{name}");
        }
    }
    fs::remove_dir_all(directory).expect("remove probes");
}

#[test]
fn map_constructor_source() {
    probe(
        "map-copy",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "const y = new Map(x);",
        21,
    );
}

#[test]
fn set_constructor_source() {
    probe(
        "set-copy",
        "const x = new Set<i32>(); x.add(2);",
        "const y = new Set<i32>(x);",
        26,
    );
}

#[test]
fn array_from_source() {
    probe(
        "array-from",
        "const x = new Set<i32>(); x.add(2);",
        "const y = Array.from(x);",
        24,
    );
}

#[test]
fn set_algebra_argument() {
    for method in [
        "union",
        "intersection",
        "difference",
        "symmetricDifference",
        "isSubsetOf",
        "isSupersetOf",
        "isDisjointFrom",
    ] {
        let operation = format!("const y = a.{method}(x);");
        let column = operation.find("(x)").unwrap() as u32 + 4;
        probe(
            method,
            "const a = new Set<i32>(); const x = new Set<i32>(); x.add(2);",
            &operation,
            column,
        );
    }
}

#[test]
fn spread_container_argument() {
    probe(
        "spread",
        "const x = new Set<i32>(); x.add(2);",
        "const y = [0, ...x];",
        20,
    );
}

#[test]
fn runtime_receiver() {
    probe(
        "receiver",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "x.has(1);",
        3,
    );
}

#[test]
fn stored_value_is_checked_only_when_read() {
    probe(
        "stored-read",
        "const a = new Map<i32, Set<i32>>(); const x = new Set<i32>(); x.add(2); a.set(1, x);",
        "a.set(2, x); const y = x.size;",
        26,
    );
}

#[test]
fn reduction_accumulator_argument() {
    for (name, setup, callback) in [
        ("static-reduce", "const a: i32[] = [1]; const x = new Set<i32>();", "(acc: Set<i32>, v: i32): Set<i32> => acc"),
        ("dynamic-reduce", "const a: i32[] = [1]; const x = new Set<i32>(); const keep = (acc: Set<i32>, v: i32): Set<i32> => acc;", "keep"),
    ] {
        let operation = format!("const y = a.reduce({callback}, x);");
        let column = operation.rfind("x)").unwrap() as u32 + 3;
        probe(name, setup, &operation, column);
    }
}

#[test]
fn worker_message_argument() {
    let directory =
        std::env::temp_dir().join(format!("subscript-lifetime-{}-worker", std::process::id()));
    fs::create_dir_all(&directory).expect("probe directory");
    for freed in [false, true] {
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("class Message {{ value: i32; constructor() {{ this.value = 1; }} }}\nfunction sink(input: Inbox<Message>, output: Outbox<Message>): void {{ input.wait(); }}\nexport function main(): void {{\n  const worker = Worker.spawn(sink); const x = new Message();\n  {release}\n  worker.post(x);\n  worker.close(); worker.join(); print(\"done\");\n}}\n");
        let path = directory.join(format!("worker-{freed}.ts"));
        fs::write(&path, source).expect("worker probe");
        let actual = Command::new(env!("CARGO_BIN_EXE_subscript"))
            .arg("run")
            .arg(&path)
            .output()
            .expect("CLI dev JIT");
        if freed {
            assert!(!actual.status.success());
            let stderr = String::from_utf8(actual.stderr).unwrap();
            assert!(
                stderr.contains(":6:15: trap [use-after-delete]:"),
                "{stderr}"
            );
        } else {
            assert!(actual.status.success(), "{actual:?}");
            assert_eq!(actual.stdout, b"done\n");
        }
    }
    fs::remove_dir_all(directory).expect("remove probes");
}

#[test]
fn synthesized_json_helper_argument() {
    let fields = "value: i32 = 1;";
    let directory =
        std::env::temp_dir().join(format!("subscript-json-lifetime-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for freed in [false, true] {
        let release = if freed { "Context.free(x);" } else { "" };
        let source = format!("class Item {{ {fields} }}\nexport function main(): void {{\n  const x = new Item();\n  {release}\n  JSON.stringify(x);\n  print(\"done\");\n}}\n");
        let path = directory.join(format!("json-{freed}.ts"));
        fs::write(&path, source).unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_subscript"))
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        if freed {
            assert!(!actual.status.success());
            assert!(actual.stdout.is_empty());
            let stderr = String::from_utf8(actual.stderr).unwrap();
            assert!(
                stderr.contains(":5:18: trap [use-after-delete]:"),
                "{stderr}"
            );
        } else {
            assert!(actual.status.success(), "{actual:?}");
            assert_eq!(actual.stdout, b"done\n");
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn nullable_json_helper_argument() {
    let fields = "value: i32 = 1;";
    let directory = std::env::temp_dir().join(format!(
        "subscript-nullable-lifetime-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    for (name, setup, release, expected) in [
        ("null", "const x: Person | null = null;", "", "null\n"),
        (
            "freed",
            "const person = new Person(); const x: Person | null = person;",
            "Context.free(person);",
            "",
        ),
    ] {
        let source = format!("class Person {{ {fields} }}\nexport function main(): void {{\n  {setup}\n  {release}\n  print(JSON.stringify(x));\n}}\n");
        let path = directory.join(format!("{name}.ts"));
        fs::write(&path, source).unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_subscript"))
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(actual.stdout, expected.as_bytes(), "{actual:?}");
        if name == "null" {
            assert!(actual.status.success(), "{actual:?}");
        } else {
            assert!(!actual.status.success(), "{actual:?}");
            let stderr = String::from_utf8(actual.stderr).unwrap();
            assert!(
                stderr.contains(":5:24: trap [use-after-delete]:"),
                "{stderr}"
            );
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn for_of_map_keys() {
    probe(
        "for-map-keys",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "for (const v of x.keys()) {}",
        19,
    );
}

#[test]
fn for_of_map_values() {
    probe(
        "for-map-values",
        "const x = new Map<i32, i32>(); x.set(1, 2);",
        "for (const v of x.values()) {}",
        19,
    );
}

#[test]
fn for_of_set_values() {
    for (name, subject) in [
        ("set", "x"),
        ("set-keys", "x.keys()"),
        ("set-values", "x.values()"),
    ] {
        probe(
            name,
            "const x = new Set<i32>(); x.add(1);",
            &format!("for (const v of {subject}) {{}}"),
            19,
        );
    }
}

#[test]
fn for_of_non_releasable_subjects() {
    let directory =
        std::env::temp_dir().join(format!("subscript-for-of-types-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for (name, setup, subject, ty) in [
        ("array-values", "const x: i32[] = [1];", "x", "i32[]"),
        ("array-keys", "const x: i32[] = [1];", "x.keys()", "i32[]"),
        (
            "fixed-array",
            "const x: FixedArray<i32, 2> = [1, 2];",
            "x",
            "FixedArray<i32, 2>",
        ),
        ("string", "const x = \"a\";", "x", "string"),
    ] {
        for freed in [false, true] {
            let release = if freed { "Context.free(x);" } else { "" };
            let source = format!("export function main(): void {{ {setup} {release} for (const v of {subject}) {{}} print(\"done\"); }}");
            let path = directory.join(format!("{name}-{freed}.ts"));
            fs::write(&path, source).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_subscript"))
                .arg("run")
                .arg(path)
                .output()
                .unwrap();
            if freed {
                let error = String::from_utf8(output.stderr).unwrap();
                assert!(!output.status.success());
                assert!(
                    error.contains(&format!("the argument expects `object`, got `{ty}`")),
                    "{name}: {error}"
                );
            } else {
                assert!(output.status.success(), "{name}: {output:?}");
                assert_eq!(output.stdout, b"done\n");
            }
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn non_storing_generic_arguments() {
    for method in ["includes", "indexOf", "lastIndexOf"] {
        let operation = format!("a.{method}(x);");
        let column = operation.find("(x)").unwrap() as u32 + 4;
        probe(
            &format!("array-{method}"),
            "const x = new Set<i32>(); const a: Set<i32>[] = [x];",
            &operation,
            column,
        );
    }
    probe(
        "map-fallback",
        "const a = new Map<i32, Set<i32>>(); const x = new Set<i32>();",
        "a.getOr(1, x);",
        14,
    );
}

#[test]
fn double_delete_keeps_the_call_position_with_async_owner_fields() {
    let directory =
        std::env::temp_dir().join(format!("subscript-release-fields-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for (name, declaration) in [
        ("owner", "async function value(): Promise<i32> { return 1; } class Holder { handle: Promise<i32>; constructor() { this.handle = value(); } }"),
        ("scalar", "class Holder { value: i32 = 1; }"),
    ] {
        for twice in [false, true] {
            let second = if twice { "Context.free(x);" } else { "" };
            let source = format!("{declaration}\nexport function main(): void {{\n  const x = new Holder();\n  Context.free(x);\n  {second}\n  print(\"done\");\n}}\n");
            let path = directory.join(format!("{name}-{twice}.ts"));
            fs::write(&path, source).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_subscript")).arg("run").arg(path).output().unwrap();
            if twice {
                assert!(!output.status.success(), "{output:?}");
                assert!(output.stdout.is_empty());
                let error = String::from_utf8(output.stderr).unwrap();
                assert!(error.contains(":5:3: trap [double-delete]: Context.free of an already-deleted allocation"), "{name}: {error}");
            } else {
                assert!(output.status.success(), "{output:?}");
                assert_eq!(output.stdout, b"done\n");
            }
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
