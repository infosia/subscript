//! Unit tests of the closed word set (§189 rule 5). The plan and the
//! report need a lowered module; `codegen/tests/crossing_plan.rs` tests
//! them on LIR.

use super::*;

/// Rule 5: each word of the closed set has its own text, so a golden line
/// names one word.
#[test]
fn each_word_has_its_own_text() {
    let texts = Word::ALL.map(Word::text);
    for (index, text) in texts.iter().enumerate() {
        assert!(!text.is_empty());
        assert!(!texts[index + 1..].contains(text), "`{text}` is two words");
    }
}

/// Rule 6: each form that code generation does not lower states its own
/// reason.
#[test]
fn each_unlowered_form_states_its_reason() {
    let reasons = [
        NotLowered::FixedArrayOfStructs,
        NotLowered::CallbackWithoutUserdata,
        NotLowered::StructCycle,
        NotLowered::WrittenBackElements,
    ]
    .map(NotLowered::reason);
    for (index, reason) in reasons.iter().enumerate() {
        assert!(!reasons[index + 1..].contains(reason), "{reason}");
    }
    assert_eq!(
        NotLowered::FixedArrayOfStructs.reason(),
        "a fixed array of structs in a struct that the call rebuilds"
    );
}

/// A call site keeps the plan that it is given.
#[test]
fn a_call_site_keeps_its_plan() {
    let plan = Arc::new(CallPlan {
        parameters: vec![ParameterPlan::Value],
        result: ResultPlan::Void,
        scratch_scope: false,
    });
    let site = CallSite::new(
        Pos::new("main.ts", 2, 3),
        ForeignFunctionId(4),
        Arc::clone(&plan),
    );
    assert_eq!(site.pos, Pos::new("main.ts", 2, 3));
    assert_eq!(site.callee, ForeignFunctionId(4));
    assert!(Arc::ptr_eq(&site.plan, &plan));
}
