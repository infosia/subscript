//! Violating native forms pass through the complete frame check.

use super::*;

fn frame_contract_fixture() -> Vec<subscript_compiler::SourceFile> {
    vec![subscript_compiler::SourceFile::new(
        "frame-contract.ts",
        r#"
class Box { n:i32; constructor(n:i32){this.n=n;} }
async function make():Promise<Box>{return new Box(8);}
async function index():Promise<i32>{return 0;}
function values():FixedArray<Box,1>{return [new Box(7)];}
async function reference():Promise<void>{
 const n=new Box(7); const f=():i32=>n.n; const g=():i32=>f()+1;
 let a:FixedArray<Box,1>=[n];
 const item=values()[await index()];
 print(`${item.n}`);
 a[0]=await make();
 print(`${a[0].n + g()}`);
}
export async function main():Promise<void>{await reference();}
"#,
    )]
}

// compiler.md §338 rule 5: debug best of three: 0.18 s; 0 C builds.
#[test]
fn missing_native_lifetime_stores_fail_the_suspension_check() {
    check_entry(&frame_contract_fixture(), "valid-write", &mut (0, 0))
        .expect("valid native writes");
    for prefix in ["env_v", "l"] {
        let removed = std::cell::RefCell::new(String::new());
        let result = check_entry_with_c(
            &frame_contract_fixture(),
            "missing-write",
            &mut (0, 0),
            &|module, source| {
                let function = module
                    .functions
                    .iter()
                    .find(|f| f.source_name == "reference")
                    .finding("fixture reference")?;
                let layouts = Layouts::build_lir(module).finding("fixture layouts")?;
                let (_, lifetime, _, _) = lir_fields(function, &layouts, true)?;
                let body = c_function(source, function.id)?.to_string();
                let name = lifetime
                    .iter()
                    .find(|name| {
                        name.starts_with(prefix)
                            && body.lines().any(|line| {
                                line.contains(&format!("frame->{name}"))
                                    && (line.contains("memset(")
                                        || line.contains(&format!("frame->{name} =")))
                            })
                    })
                    .finding("fixture lifetime field")?;
                *removed.borrow_mut() = name.clone();
                let corrupted = body
                    .lines()
                    .filter(|line| {
                        !(line.contains(&format!("frame->{name}"))
                            && (line.contains("memset(")
                                || line.contains("memcpy(")
                                || line.contains("->c")
                                || line.contains(&format!("frame->{name} ="))))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                *source = source.replacen(&body, &corrupted, 1);
                Ok(())
            },
        );
        let message = result.expect_err("a missing native write must fail");
        eprintln!("omitted {prefix}: {message}");
        let name = removed.into_inner();
        assert!(!name.is_empty(), "the fixture must remove a field");
        assert!(
            message.ends_with(&format!("missing write {name}")),
            "removed {name}: {message}"
        );
    }
}

// compiler.md §338 rule 5: debug best of three: 0.11 s; 0 C builds.
#[test]
fn a_block_parameter_or_edge_argument_clear_fails_the_check() {
    let source = "async function tick():Promise<void>{} export async function main():Promise<void>{const n:i32=7;let f=():i32=>n;for(let i:i32=0;i<2;i+=1){await tick();print(`${f()}`);f=():i32=>n+1;}}";
    let files = [subscript_compiler::SourceFile::new("edge-clear.ts", source)];
    check_entry(&files, "valid-clear", &mut (0, 0)).expect("unchanged fixture passes");
    for edge in [false, true] {
        let changed = std::cell::RefCell::new(None);
        let result = check_entry_with_c(
            &files,
            "premature-clear",
            &mut (0, 0),
            &|module, source| {
                let function = module
                    .functions
                    .iter()
                    .find(|f| f.source_name == "main")
                    .finding("fixture main")?;
                let candidate = function.blocks.iter().find_map(|block| {
                if edge {
                    block.terminator.targets().iter().filter(|_| !matches!(block.terminator, l::Terminator::Suspend { .. }))
                        .flat_map(|target| target.arguments.iter().filter_map(|operand| match operand { l::Operand::Value(value) => Some(*value), _ => None }).collect::<Vec<_>>())
                        .find(|id| matches!(&function.values[id.0 as usize].ty, l::ValueType::Data(ty) if ty.function_type().is_some())
                            && !lifetime_assignment(function, &format!("env_v{}", id.0), block, block.instructions.len() + 1).unwrap())
                        .map(|id| (block, id, block.instructions.len() + 1))
                } else {
                    block.parameters.iter().find(|id| matches!(&function.values[id.0 as usize].ty, l::ValueType::Data(ty) if ty.function_type().is_some()))
                        .map(|id| (block, *id, block.instructions.len().max(1)))
                }
            }).finding("fixture phi or edge argument")?;
                let (block, value, point) = candidate;
                *changed.borrow_mut() = Some((format!("env_v{}", value.0), (block.id.0, point)));
                let body = c_function(source, function.id)?.to_string();
                let marker = format!("// root-point {} {point}\n", block.id.0);
                require!(body.contains(&marker), "fixture point");
                let clear = format!(
                    "{marker}    memset(&frame->env_v{}, 0, sizeof frame->env_v{});\n",
                    value.0, value.0
                );
                *source = source.replacen(&body, &body.replacen(&marker, &clear, 1), 1);
                Ok(())
            },
        );
        let message = result.expect_err("a premature clear must fail");
        eprintln!("premature edge={edge}: {message}");
        let (field, point) = changed.into_inner().expect("the fixture changes one field");
        assert_eq!(
            message,
            format!("premature-clear: C lifetime clear {field} at {point:?}: final operation Some(Zero)"),
            "edge={edge}"
        );
    }
}

// compiler.md §338 rule 5: debug best of three: 0.05 s; 0 C builds.
#[test]
fn each_c_finish_exit_requires_its_own_payload_clear() {
    let files = [subscript_compiler::SourceFile::new(
        "finish-clear.ts",
        "export async function main():Promise<void>{await Context.suspend();}",
    )];
    check_entry(&files, "valid-finish-clear", &mut (0, 0)).expect("unchanged fixture passes");
    for exit in 0..2 {
        let changed_exit = std::cell::Cell::new(None);
        let result = check_entry_with_c(&files, "finish-clear", &mut (0, 0), &|module, source| {
            let function = module
                .functions
                .iter()
                .find(|f| f.source_name == "main")
                .finding("fixture main")?;
            let body = c_function(source, function.id)?.to_string();
            let mut count = 0;
            let mut pending_exit = false;
            let mut lines = Vec::new();
            for line in body.lines() {
                if line.contains("memset((unsigned char*)frame + sizeof frame->state") {
                    let omit = count == exit;
                    count += 1;
                    if omit {
                        pending_exit = true;
                        continue;
                    }
                }
                lines.push(line);
                if pending_exit && line.trim() == "return 1;" {
                    changed_exit.set(Some(lines.len()));
                    pending_exit = false;
                }
            }
            let corrupted = lines.join("\n");
            require_eq!(count, 2, "fixture finish paths");
            *source = source.replacen(&body, &corrupted, 1);
            Ok(())
        });
        let error = result.expect_err("each exit needs a clear");
        eprintln!("finish exit {exit}: {error}");
        let line = changed_exit
            .get()
            .expect("the omitted clear has a return exit");
        assert_eq!(
            error,
            format!("finish-clear: C finish exit without payload clear at line {line}"),
            "exit={exit}"
        );
    }
}

// compiler.md §338 rule 5: debug best of three: 0.19 s; 0 C builds.
#[test]
fn a_missing_stable_storage_write_fails_the_suspension_check() {
    let mut module =
        crate::lir::lower_module(&corpus::check_program(&frame_contract_fixture()).unwrap())
            .unwrap();
    let layouts = Layouts::build_lir(&module).unwrap();
    let mut function = module
        .functions
        .iter()
        .find(|f| f.source_name == "reference")
        .unwrap()
        .clone();
    let array = function
        .values
        .iter()
        .find(|v| matches!(v.ty, l::ValueType::Data(Type::FixedArray(_, 1))))
        .unwrap()
        .id;
    let l::ValueType::Data(pointee) = function.values[array.0 as usize].ty.clone() else {
        unreachable!()
    };
    let address = l::ValueId(function.values.len() as u32);
    let parameter = l::ValueId(address.0 + 1);
    let ty = l::ValueType::Address(l::AddressType {
        pointee: pointee.clone(),
        array_base: None,
    });
    for id in [address, parameter] {
        function.values.push(l::Value {
            id,
            ty: ty.clone(),
            fresh_owner: false,
            source_name: None,
        });
    }
    let owner_parameter = l::ValueId(parameter.0 + 1);
    function.values.push(l::Value {
        id: owner_parameter,
        ty: l::ValueType::Data(pointee),
        fresh_owner: false,
        source_name: None,
    });
    let block = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, l::Terminator::Suspend { .. }))
        .unwrap();
    let id = block.id;
    let point = block.instructions.len() + 1;
    block.instructions.push(l::Instruction {
        result: Some(address),
        kind: l::InstructionKind::AddressOfValue,
        count_action: None,
        operands: vec![l::Operand::Value(array)],
        invalidates: vec![],
        traps: vec![],
        pos: function.pos.clone(),
    });
    let l::Terminator::Suspend {
        successor,
        arguments,
        ownership,
        ..
    } = &mut block.terminator
    else {
        unreachable!()
    };
    arguments.extend([l::Operand::Value(address), l::Operand::Value(array)]);
    ownership.extend([false, false]);
    let successor = *successor;
    function.blocks[successor.0 as usize]
        .parameters
        .extend([parameter, owner_parameter]);
    function
        .liveness
        .value_origins
        .extend([address, address, array]);
    module.functions[function.id.0 as usize] = function.clone();
    crate::lir::verify_module(&module).expect("valid LIR before the native write is omitted");
    let function = &function;
    let (_, lifetime, _, _) = lir_fields(function, &layouts, true).unwrap();
    let name = format!("stable_v{}", address.0);
    assert!(lifetime.contains(&name));
    check_module_with_forms(
        &module,
        "valid-stable",
        &mut (0, 0),
        &|_, _| Ok(()),
        &|_, _| Ok(()),
    )
    .unwrap();
    for jit in [false, true] {
        let c = |_: &l::Module, source: &mut String| {
            if jit {
                return Ok(());
            }
            let body = c_function(source, function.id)?.to_string();
            let mut removed = 0;
            let changed = body
                .lines()
                .filter(|line| {
                    let omit = line.contains(&format!("memcpy(&frame->{name},"))
                        || line.contains(&format!("frame->{name} ="));
                    removed += usize::from(omit);
                    !omit
                })
                .collect::<Vec<_>>()
                .join("\n");
            require!(removed > 0, "fixture stable C write");
            *source = source.replacen(&body, &changed, 1);
            Ok(())
        };
        let native = |module: &l::Module, texts: &mut NativeTexts| {
            if !jit {
                return Ok(());
            }
            let field = native_fields(module, function)
                .into_iter()
                .find(|field| field.name == name)
                .unwrap();
            omit_jit_write(
                texts
                    .get_mut(&format!("LIR coroutine resume {}", function.id.0))
                    .unwrap(),
                &field,
                (id.0, point),
            );
            Ok(())
        };
        let error = check_module_with_forms(&module, "stable-write", &mut (0, 0), &c, &native)
            .expect_err("a missing stable write must fail the full check");
        eprintln!("{error}");
        assert!(
            error.contains(if jit {
                "JIT assignment"
            } else {
                "C assignment"
            }) && error.ends_with(&format!("missing write {name}")),
            "{error}"
        );
    }
}

// compiler.md §338 rule 5: debug best of three: 0.13 s; 0 C builds.
#[test]
fn a_jit_parameter_write_on_only_one_incoming_edge_fails() {
    let files = fixture(LOOP_STORE);
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    for omit in [false, true] {
        let error = check_jit(&files, &|module, texts| {
            let function = main_function(module);
            let fields = native_fields(module, function);
            let (target, parameter, incoming) = loop_parameter(function, &fields);
            let name = format!("env_v{}", parameter.0);
            let field = fields.iter().find(|field| field.name == name).unwrap();
            assert_eq!(field.size, 8);
            let text = texts
                .get_mut(&format!("LIR coroutine resume {}", function.id.0))
                .unwrap();
            omit_jit_write(text, field, (target.0, 0));
            for (index, (source, argument)) in incoming.iter().enumerate() {
                if omit && index == 1 {
                    continue;
                }
                let source_field = fields
                    .iter()
                    .find(|field| field.name == format!("env_v{}", argument.0))
                    .unwrap();
                let point = (source.id.0, source.instructions.len() + 1);
                let tag = (point.0 + 1) * 65536 + point.1 as u32;
                let value = 910000 + index;
                let code = format!(
                    "@{tag:x} v{value} = load.i64 notrap v1+{}\n@{tag:x} store v{value}, v1+{}",
                    source_field.offset, field.offset
                );
                insert_jit_at_end(text, point, code);
            }
            Ok(())
        });
        if omit {
            let error = error.expect_err("one edge write cannot cover the other edge");
            eprintln!("{error}");
            assert!(
                error.contains("JIT edge b4 -> b1 missing write env_v2"),
                "{error}"
            );
        } else {
            error.expect("each edge writes through the full native check");
        }
    }
}

fn loop_parameter<'a>(
    function: &'a l::Function,
    fields: &[Field],
) -> (l::BlockId, l::ValueId, Vec<(&'a l::BasicBlock, l::ValueId)>) {
    function
        .blocks
        .iter()
        .find_map(|target| {
            target
                .parameters
                .iter()
                .enumerate()
                .find_map(|(index, parameter)| {
                    if !fields
                        .iter()
                        .any(|field| field.name == format!("env_v{}", parameter.0))
                    {
                        return None;
                    }
                    let incoming = function
                        .blocks
                        .iter()
                        .filter(|source| {
                            !matches!(source.terminator, l::Terminator::Suspend { .. })
                        })
                        .flat_map(|source| {
                            source
                                .terminator
                                .targets()
                                .into_iter()
                                .filter(|edge| edge.block == target.id)
                                .filter_map(move |edge| {
                                    if let Some(l::Operand::Value(value)) =
                                        edge.arguments.get(index)
                                    {
                                        Some((source, *value))
                                    } else {
                                        None
                                    }
                                })
                        })
                        .collect::<Vec<_>>();
                    (incoming.len() == 2).then_some((target.id, *parameter, incoming))
                })
        })
        .expect("a loop parameter with entry and back edges")
}

fn insert_jit_at_end(text: &mut String, point: Point, code: String) {
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let insert = lines
        .iter()
        .rposition(|line| {
            native_point(line) == Some(point)
                && !line.contains("jump ")
                && !line.contains("brif ")
                && !line.contains("return ")
        })
        .expect("native point")
        + 1;
    lines.insert(insert, code);
    *text = lines.join("\n");
}

const SECOND_STORE: &str = "class Box{n:i32;constructor(n:i32){this.n=n;}} export async function main():Promise<void>{let b:FixedArray<Box,1>=[new Box(1)];await Context.suspend();print(`${b[0].n}`);b=[new Box(2)];await Context.suspend();print(`${b[0].n}`);}";
const LOOP_STORE: &str = "class Box{n:i32;constructor(n:i32){this.n=n;}} export async function main():Promise<void>{const initial=new Box(7);let f=():i32=>initial.n;let i:i32=0;while(i<2){await Context.suspend();print(`${f()}`);const local=new Box(i);f=():i32=>local.n;i+=1;}}";

fn fixture(source: &str) -> Vec<subscript_compiler::SourceFile> {
    vec![subscript_compiler::SourceFile::new(
        "native-contract.ts",
        source,
    )]
}

fn main_function(module: &l::Module) -> &l::Function {
    module
        .functions
        .iter()
        .find(|f| f.source_name == "main")
        .unwrap()
}

fn native_fields(module: &l::Module, function: &l::Function) -> Vec<Field> {
    let layouts = Layouts::build_lir(module).unwrap();
    let plan = plan_coroutine(&layouts, module, function).unwrap();
    let size = closure_environment_layout(module, &layouts)
        .unwrap()
        .map(|(size, _)| size);
    jit_fields(function, &plan, &layouts, size).unwrap().1
}

fn native_point(line: &str) -> Option<Point> {
    let tag = line.trim().strip_prefix('@')?.split_whitespace().next()?;
    let tag = u32::from_str_radix(tag, 16).ok()?;
    Some((tag / 65536 - 1, (tag % 65536) as usize))
}

fn initialized(text: &str, field: &Field, point: Point) -> bool {
    let facts = jit_byte_facts(text, "v1", 0).unwrap();
    let mut bytes = facts.positive;
    for (point, zero) in facts.zero {
        bytes.entry(point).or_default().extend(zero);
    }
    field_bytes(&bytes, std::slice::from_ref(field))
        .get(&point)
        .is_some_and(|fields| fields.contains(&field.name))
}

// Remove an actual store or copy operation, then require absence of the whole field at this point.
fn omit_jit_write(text: &mut String, field: &Field, point: Point) {
    assert!(
        initialized(text, field, point),
        "fixture writes {} at {point:?}",
        field.name
    );
    let lines = text.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if native_point(line) != Some(point) || !(line.contains("store") || line.contains("call "))
        {
            continue;
        }
        let candidate = lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, line)| *line)
            .collect::<Vec<_>>()
            .join("\n");
        if !initialized(&candidate, field, point) {
            *text = candidate;
            return;
        }
    }
    panic!(
        "fixture has one whole-field store for {} at {point:?}",
        field.name
    );
}

fn second_local(function: &l::Function) -> (String, Vec<Point>) {
    let mut points = BTreeMap::<String, Vec<Point>>::new();
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if function.locals[local.0 as usize].storage == l::LocalStorageClass::Frame {
                    points
                        .entry(format!("l{}", local.0))
                        .or_default()
                        .push((block.id.0, index + 1));
                }
            }
        }
    }
    points
        .into_iter()
        .find(|(_, points)| points.len() >= 2)
        .expect("a frame local with two assignments")
}

fn check_jit(
    files: &[subscript_compiler::SourceFile],
    mutate: &NativeMutation<'_>,
) -> Result<(), String> {
    check_entry_with_forms(files, "jit-negative", &mut (0, 0), &|_, _| Ok(()), mutate)
}

// compiler.md §338 rule 5: debug best of three: 0.06 s; 0 C builds.
#[test]
fn a_whole_jit_field_write_missing_fails_the_full_check() {
    let files = fixture(SECOND_STORE);
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    let error = check_jit(&files, &|module, texts| {
        let function = main_function(module);
        let (name, points) = second_local(function);
        let field = native_fields(module, function)
            .into_iter()
            .find(|f| f.name == name)
            .unwrap();
        let text = texts
            .get_mut(&format!("LIR coroutine resume {}", function.id.0))
            .unwrap();
        for point in points {
            omit_jit_write(text, &field, point);
        }
        Ok(())
    })
    .expect_err("a whole missing JIT field must fail");
    eprintln!("{error}");
    assert!(
        error.contains("JIT assignment") && error.contains("missing write l"),
        "{error}"
    );
}

// compiler.md §338 rule 5: debug best of three: 0.09 s; 0 C builds.
#[test]
fn a_second_assignment_requires_a_fresh_native_write_in_each_tier() {
    let files = fixture(SECOND_STORE);
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    for jit in [false, true] {
        let c = |module: &l::Module, source: &mut String| {
            if jit {
                return Ok(());
            }
            let function = main_function(module);
            let (name, points) = second_local(function);
            let point = points[1];
            let body = c_function(source, function.id)?.to_string();
            let mut current = (u32::MAX, 0);
            let mut removed = 0;
            let changed = body
                .lines()
                .filter(|line| {
                    if let Some(marker) = line.trim().strip_prefix("// root-point ") {
                        let mut parts = marker.split_whitespace();
                        current = (
                            parts.next().unwrap().parse().unwrap(),
                            parts.next().unwrap().parse().unwrap(),
                        );
                    }
                    let omit = current == point && line.contains(&format!("frame->{name} ="));
                    removed += usize::from(omit);
                    !omit
                })
                .collect::<Vec<_>>()
                .join("\n");
            require_eq!(removed, 1, "one second C assignment");
            *source = source.replacen(&body, &changed, 1);
            Ok(())
        };
        let native = move |module: &l::Module, texts: &mut NativeTexts| {
            if !jit {
                return Ok(());
            }
            let function = main_function(module);
            let (name, points) = second_local(function);
            let field = native_fields(module, function)
                .into_iter()
                .find(|f| f.name == name)
                .unwrap();
            omit_jit_write(
                texts
                    .get_mut(&format!("LIR coroutine resume {}", function.id.0))
                    .unwrap(),
                &field,
                points[1],
            );
            Ok(())
        };
        let error = check_entry_with_forms(&files, "second-store", &mut (0, 0), &c, &native)
            .expect_err("each assignment needs a fresh write");
        eprintln!("{error}");
        assert!(
            error.contains(if jit {
                "JIT assignment"
            } else {
                "C assignment"
            }) && error.contains("missing write l"),
            "{error}"
        );
    }
}

// compiler.md §338 rule 5: debug best of three: 0.07 s; 0 C builds.
#[test]
fn a_c_back_edge_parameter_requires_its_own_write() {
    let files = fixture(LOOP_STORE);
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    let error = check_entry_with_c(&files, "c-back-edge", &mut (0, 0), &|module, source| {
        let function = main_function(module);
        let (target, parameter, incoming) = function
            .blocks
            .iter()
            .find_map(|target| {
                target.parameters.iter().find_map(|parameter| {
                    let name = format!("env_v{}", parameter.0);
                    let fields = native_fields(module, function);
                    if !fields.iter().any(|f| f.name == name) {
                        return None;
                    }
                    let incoming = function
                        .blocks
                        .iter()
                        .filter(|source| {
                            !matches!(source.terminator, l::Terminator::Suspend { .. })
                                && source.terminator.successors().contains(&target.id)
                        })
                        .collect::<Vec<_>>();
                    (incoming.len() == 2).then_some((target, *parameter, incoming))
                })
            })
            .unwrap();
        let back = incoming
            .iter()
            .find(|source| source.id.0 > target.id.0)
            .unwrap();
        let point = (back.id.0, back.instructions.len() + 1);
        let body = c_function(source, function.id)?.to_string();
        let marker = format!("// root-point {} {}", point.0, point.1);
        let (before, edge) = body.split_once(&marker).unwrap();
        let name = format!("env_v{}", parameter.0);
        let mut removed = 0;
        let changed = edge
            .lines()
            .filter(|line| {
                let omit = line.contains(&format!("&frame->{name},")) && line.contains("memcpy(");
                removed += usize::from(omit);
                !omit
            })
            .collect::<Vec<_>>()
            .join("\n");
        require_eq!(removed, 1, "one back-edge C environment copy");
        *source = source.replacen(&body, &format!("{before}{marker}{changed}"), 1);
        Ok(())
    })
    .expect_err("the C back edge needs its own write");
    eprintln!("{error}");
    assert!(
        error.contains("C assignment") && error.contains("missing write env_v"),
        "{error}"
    );
}

// compiler.md §338 rule 5: debug best of three: 0.11 s; 0 C builds.
#[test]
fn jit_block_parameter_and_edge_argument_clears_fail_the_full_check() {
    let files = fixture(LOOP_STORE);
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    for edge in [false, true] {
        let error = check_jit(&files, &move |module, texts| {
            let function = main_function(module);
            let fields = native_fields(module, function);
            let (field, point) = function
                .blocks
                .iter()
                .find_map(|block| {
                    let point = (
                        block.id.0,
                        if edge {
                            block.instructions.len() + 1
                        } else {
                            block.instructions.len().max(1)
                        },
                    );
                    let values = if edge {
                        if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                            return None;
                        }
                        block
                            .terminator
                            .targets()
                            .iter()
                            .flat_map(|target| {
                                target
                                    .arguments
                                    .iter()
                                    .filter_map(|operand| {
                                        if let l::Operand::Value(value) = operand {
                                            Some(*value)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()
                    } else {
                        block.parameters.clone()
                    };
                    values.into_iter().find_map(|value| {
                        let field = fields
                            .iter()
                            .find(|f| f.name == format!("env_v{}", value.0))?;
                        (!lifetime_assignment(function, &field.name, block, point.1).unwrap())
                            .then_some((field, point))
                    })
                })
                .expect("a block parameter or edge argument outside its assignment point");
            let text = texts
                .get_mut(&format!("LIR coroutine resume {}", function.id.0))
                .unwrap();
            let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
            let insert = lines
                .iter()
                .rposition(|line| {
                    native_point(line) == Some(point)
                        && !line.contains("jump ")
                        && !line.contains("brif ")
                        && !line.contains("return ")
                })
                .expect("native LIR point")
                + 1;
            let tag = (point.0 + 1) * 65536 + point.1 as u32;
            let mut clear = format!(
                "@{tag:x} v900000 = iadd_imm.i64 v1, {}\n@{tag:x} v900001 = iconst.i64 0\n",
                field.offset
            );
            for offset in (0..field.size).step_by(8) {
                clear.push_str(&format!("@{tag:x} store v900001, v900000+{offset}\n"));
            }
            lines.insert(insert, clear);
            *text = lines.join("\n");
            Ok(())
        })
        .expect_err("a premature JIT clear must fail");
        eprintln!("edge={edge}: {error}");
        assert!(error.contains("JIT lifetime clear env_v"), "{error}");
    }
}

fn omit_finish_clear(text: &mut String, point: Point, offset: u32) {
    let mut removed = 0;
    loop {
        let before = jit_byte_facts(text, "v1", 0)
            .unwrap()
            .zero
            .remove(&point)
            .unwrap_or_default();
        let lines = text.lines().collect::<Vec<_>>();
        let candidate = lines.iter().enumerate().find_map(|(index, line)| {
            if native_point(line) != Some(point)
                || !(line.contains("store") || line.contains("call "))
            {
                return None;
            }
            let changed = lines
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n");
            let after = jit_byte_facts(&changed, "v1", 0)
                .unwrap()
                .zero
                .remove(&point)
                .unwrap_or_default();
            before
                .difference(&after)
                .any(|byte| *byte >= offset)
                .then_some(changed)
        });
        if let Some(changed) = candidate {
            *text = changed;
            removed += 1;
        } else {
            break;
        }
    }
    assert!(removed > 0, "a finish clear at {point:?}");
}

// compiler.md §338 rule 5: debug best of three: 0.14 s; 0 C builds.
#[test]
fn each_jit_return_and_unwind_requires_its_own_finish_clear() {
    let files = fixture("class Box{n:i32;constructor(n:i32){this.n=n;}} export async function main():Promise<void>{let b=new Box(1);await Context.suspend();if(b.n==1){return;}print(`${b.n}`);}");
    check_entry(&files, "valid", &mut (0, 0)).unwrap();
    for exit_index in 0..3 {
        let removed = std::cell::Cell::new((0, 0));
        let error = check_jit(&files, &|module, texts| {
            let function = main_function(module);
            let mut exits = function
                .blocks
                .iter()
                .filter(|b| matches!(b.terminator, l::Terminator::Return { .. }))
                .map(|b| (b.id.0, 65534))
                .collect::<Vec<_>>();
            assert_eq!(exits.len(), 2, "two return paths");
            exits.push((65532, 1));
            let point = exits[exit_index];
            removed.set(point);
            let text = texts
                .get_mut(&format!("LIR coroutine resume {}", function.id.0))
                .unwrap();
            omit_finish_clear(text, point, payload_offset(function));
            Ok(())
        })
        .expect_err("each JIT exit needs its own clear");
        eprintln!("{error}");
        assert!(
            error.contains(&format!(
                "JIT finish exit {:?} without payload clear",
                removed.get()
            )),
            "{error}"
        );
    }
    let error = check_jit(&files, &|module, texts| {
        let function = main_function(module);
        let text = texts
            .get_mut(&format!("LIR coroutine resume {}", function.id.0))
            .unwrap();
        assert!(text.contains("@fffd0001"));
        *text = text.replace("@fffd0001", "");
        Ok(())
    })
    .expect_err("an untagged unwind return must fail");
    eprintln!("{error}");
    assert!(
        error.contains("JIT already-done dispatch must be the only untagged return"),
        "{error}"
    );
}

fn clear_at_assignment(entry: bool, jit: bool, before: bool) -> Result<(), String> {
    let files = fixture(if entry { LOOP_STORE } else { SECOND_STORE });
    let select = |module: &l::Module| {
        let function = main_function(module);
        let fields = native_fields(module, function);
        if entry {
            let (target, parameter, _) = loop_parameter(function, &fields);
            (
                fields
                    .into_iter()
                    .find(|f| f.name == format!("env_v{}", parameter.0))
                    .unwrap(),
                (target.0, 0),
            )
        } else {
            let (name, points) = second_local(function);
            (
                fields.into_iter().find(|f| f.name == name).unwrap(),
                points[0],
            )
        }
    };
    let c = |module: &l::Module, source: &mut String| {
        if jit {
            return Ok(());
        }
        let function = main_function(module);
        let (field, point) = select(module);
        let body = c_function(source, function.id)?.to_string();
        let clear = format!("    memset(&frame->{0}, 0, sizeof frame->{0});", field.name);
        let mut current = (u32::MAX, 0);
        let mut inserted = 0;
        let mut changed = Vec::new();
        for line in body.lines() {
            if let Some(marker) = line.trim().strip_prefix("// root-point ") {
                let mut parts = marker.split_whitespace();
                current = (
                    parts.next().unwrap().parse().unwrap(),
                    parts.next().unwrap().parse().unwrap(),
                );
            }
            let insert = if entry {
                line.trim() == format!("// root-point {} 0", point.0)
            } else {
                current == point && line.contains(&format!("frame->{} =", field.name))
            };
            if insert && before {
                changed.push(clear.as_str());
            }
            changed.push(line);
            if insert && !before {
                changed.push(clear.as_str());
            }
            inserted += usize::from(insert);
        }
        require_eq!(inserted, 1, "one C assignment clear");
        *source = source.replacen(&body, &changed.join("\n"), 1);
        Ok(())
    };
    let native = |module: &l::Module, texts: &mut NativeTexts| {
        if !jit {
            return Ok(());
        }
        let function = main_function(module);
        let (field, point) = select(module);
        let text = texts
            .get_mut(&format!("LIR coroutine resume {}", function.id.0))
            .unwrap();
        let tag = (point.0 + 1) * 65536 + point.1 as u32;
        let mut clear = format!("@{tag:x} v920000 = iconst.i64 0\n");
        for offset in (field.offset..field.offset + field.size).step_by(8) {
            clear.push_str(&format!("@{tag:x} store v920000, v1+{offset}\n"));
        }
        if before {
            let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
            let insert = lines
                .iter()
                .position(|line| native_point(line) == Some(point))
                .unwrap();
            lines.insert(insert, clear);
            *text = lines.join("\n");
        } else {
            insert_jit_at_end(text, point, clear);
        }
        Ok(())
    };
    check_entry_with_forms(&files, "ordered-clear", &mut (0, 0), &c, &native)
}

// compiler.md §338 rule 5: debug best of three: 0.15 s; 0 C builds.
#[test]
fn an_entry_clear_after_a_parameter_write_fails_in_both_tiers() {
    check_entry(&fixture(LOOP_STORE), "valid-entry", &mut (0, 0)).unwrap();
    clear_at_assignment(true, true, true).expect("a JIT initialization before the parameter write");
    for jit in [false, true] {
        let error = clear_at_assignment(true, jit, false)
            .expect_err("a receiving-block clear after its parameter write must fail");
        eprintln!("{error}");
        assert!(
            error.contains(if jit {
                "JIT lifetime clear env_v2 at (1, 0)"
            } else {
                "C lifetime clear env_v2 at (1, 0)"
            }),
            "{error}"
        );
        assert!(error.contains("final operation Some(Zero)"), "{error}");
    }
}

// compiler.md §338 rule 5: debug best of three: 0.14 s; 0 C builds.
#[test]
fn an_instruction_clear_after_its_write_fails_in_both_tiers() {
    check_entry(&fixture(SECOND_STORE), "valid-instruction", &mut (0, 0)).unwrap();
    for jit in [false, true] {
        clear_at_assignment(false, jit, true)
            .expect("zero initialization before the instruction write");
        let error = clear_at_assignment(false, jit, false)
            .expect_err("a clear after the instruction write must fail");
        eprintln!("{error}");
        assert!(
            error.contains(if jit {
                "JIT lifetime clear l0 at (0, 4)"
            } else {
                "C lifetime clear l0 at (0, 4)"
            }),
            "{error}"
        );
        assert!(error.contains("final operation Some(Zero)"), "{error}");
    }
}

// compiler.md §338 rule 5: debug best of three: 0.03 s; 0 C builds.
#[test]
fn a_lir_null_store_is_a_native_write_in_both_tiers() {
    let mut module =
        crate::lir::lower_module(&corpus::check_program(&fixture(SECOND_STORE)).unwrap()).unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .unwrap();
    let class = function
        .values
        .iter()
        .find_map(|value| match &value.ty {
            l::ValueType::Data(Type::Class(class)) => Some(Type::Class(*class)),
            _ => None,
        })
        .unwrap();
    let ty = Type::Nullable(Box::new(class), subscript_compiler::Absence::Null);
    let local = l::LocalId(function.locals.len() as u32);
    function.locals.push(l::Local {
        id: local,
        source_name: "nullable".into(),
        ty: l::ValueType::Data(ty.clone()),
        mutable: true,
        storage: l::LocalStorageClass::Frame,
        pos: function.pos.clone(),
    });
    let block = function
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
        .unwrap();
    block.instructions.push(l::Instruction {
        result: None,
        kind: l::InstructionKind::StoreLocal(local),
        count_action: None,
        operands: vec![l::Operand::Constant(l::Constant {
            ty,
            kind: l::ConstantKind::Null,
        })],
        invalidates: vec![],
        traps: vec![],
        pos: function.pos.clone(),
    });
    crate::lir::verify_module(&module)
        .expect("valid null-operand frame store before native lowering");
    check_module_with_forms(
        &module,
        "null-store",
        &mut (0, 0),
        &|module, source| {
            let function = main_function(module);
            let fields = native_fields(module, function);
            let nulls = ordered_operations::null_assignments(function);
            let facts = c_field_facts(
                c_function(source, function.id)?,
                &fields,
                Some(function.entry),
                &BTreeMap::new(),
            )?;
            require!(
                nulls
                    .iter()
                    .any(
                        |(point, names)| facts.operations.get(point).is_some_and(|operations| {
                            operations.iter().any(|(name, operation)| {
                                names.contains(name) && *operation == Operation::Zero
                            })
                        })
                    ),
                "a C zero store with an independent LIR null operand"
            );
            Ok(())
        },
        &|module, texts| {
            let function = main_function(module);
            let fields = native_fields(module, function);
            let nulls = ordered_operations::null_assignments(function);
            let facts = jit_byte_facts(
                &texts[&format!("LIR coroutine resume {}", function.id.0)],
                "v1",
                0,
            )?;
            let zero = field_bytes(&facts.zero, &fields);
            require!(
                nulls.iter().any(|(point, names)| zero
                    .get(point)
                    .is_some_and(|fields| !names.is_disjoint(fields))),
                "a JIT zero store with an independent LIR null operand"
            );
            Ok(())
        },
    )
    .expect("an actual LIR null store is a legitimate native zero write");
}
