use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn floating_generic_async_call_is_rejected() {
    let source = r#"
async function first<T>(items: T[]): Promise<T> {
  await Context.suspend();
  return items[0];
}

export function main(): void {
  const items: u32[] = [1];
  first<u32>(items);
}
"#;
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("the floating async call must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S013);
    assert_eq!(
        diagnostics[0].message,
        "an async handle is dropped without any await of its completion"
    );
}

#[test]
fn awaited_generic_async_call_requires_type_arguments() {
    let source = r#"
async function first<T>(items: T[]): Promise<T> {
  await Context.suspend();
  return items[0];
}

export async function main(): Promise<void> {
  const items: u32[] = [1];
  await first(items);
}
"#;
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("the call without type arguments must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "generic function `first` requires explicit type arguments"
    );
}

#[test]
fn generic_value_class_rejects_an_async_method_at_instantiation() {
    let source = r#"
@CStruct
class Box<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }

  async read(): Promise<T> {
    await Context.suspend();
    return this.value;
  }
}

export function main(): void {
  const value: Box<u32> = new Box<u32>(1);
}
"#;
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("the value-class async method must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "async methods on `@CStruct` value classes are not in the decided surface"
    );
}

#[test]
fn exported_generic_async_instance_is_not_exported_in_hir() {
    let source = r#"
export async function go<T>(): Promise<void> {
  await Context.suspend();
}

export async function main(): Promise<void> {
  await go<u32>();
}
"#;
    let module = check_program(&[SourceFile::new("test.ts", source)])
        .expect("the generic async program must check");
    let go = module
        .functions
        .iter()
        .find(|function| function.name == "go<u32>")
        .expect("go<u32> instance");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");

    assert!(go.is_async);
    assert!(!go.exported);
    assert!(main.is_async);
    assert!(main.exported);
}

#[test]
fn generic_async_function_and_method_instances_are_in_hir() {
    let source = r#"
class Box<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }

  async read(): Promise<T> {
    await Context.suspend();
    return this.value;
  }
}

async function first<T>(items: T[]): Promise<T> {
  await Context.suspend();
  return items[0];
}

export async function main(): Promise<void> {
  const box: Box<u32> = new Box<u32>(7);
  const value: u32 = await box.read();
  const items: u32[] = [value];
  const item: u32 = await first<u32>(items);
  print(`${item}`);
}
"#;
    let module = check_program(&[SourceFile::new("test.ts", source)])
        .expect("the generic async program must check");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "first<u32>")
        .expect("first<u32> instance");
    let class = module
        .classes
        .iter()
        .find(|class| class.name == "Box<u32>")
        .expect("Box<u32> instance");
    let method = class
        .methods
        .iter()
        .find(|method| method.name == "read")
        .expect("Box<u32>.read method");

    assert!(function.is_async);
    assert!(method.is_async);
}
