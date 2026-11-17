//! Paths shared by both tiers' pointer diagnostics (compiler.md §325 rule 6).
use super::*;
pub(super) fn parameter(module: &l::Module, plan: &mut ParameterPlan, path: &str) {
    match plan {
        ParameterPlan::Pointer(value) => pointer(module, value, path),
        ParameterPlan::ByValue(value) => structure(module, value, path),
        ParameterPlan::Pair(value) => pair(module, value, path),
        ParameterPlan::FixedArray(value) => {
            if let Some(value) = &mut value.element {
                pointer(module, value, path);
            }
        }
        _ => {}
    }
}
fn pointer(module: &l::Module, value: &mut PointerPlan, path: &str) {
    value.path = path.into();
    if let Some(target) = &mut value.target {
        structure(module, target, path);
    }
    if let Some(write) = &mut value.write_back {
        write_back(module, write, path);
    }
}
fn structure(module: &l::Module, value: &mut StructPlan, path: &str) {
    value.path = path.into();
    for member in &mut value.members {
        let path = format!(
            "{path}.{}",
            module.classes[value.class.0].fields[member.field].source_name
        );
        match &mut member.crossing {
            MemberCrossing::Pointer(value) => pointer(module, value, &path),
            MemberCrossing::Embedded(value) => structure(module, value, &path),
            MemberCrossing::Pair(value) => pair(module, value, &path),
            _ => {}
        }
    }
}
fn pair(module: &l::Module, value: &mut PairPlan, path: &str) {
    value.path = path.into();
    if let Some(value) = &mut value.elements {
        if let Some(value) = &mut value.element {
            structure(module, value, &format!("{path}[]"));
        }
    }
}
fn write_back(module: &l::Module, value: &mut WriteBackPlan, path: &str) {
    value.path = path.into();
    let class = value.class;
    for member in &mut value.members {
        if let WriteBack::Embedded(value) = &mut member.write {
            let path = format!(
                "{path}.{}",
                module.classes[class.0].fields[member.field].source_name
            );
            write_back(module, value, &path);
        }
    }
}
