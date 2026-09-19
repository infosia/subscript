//! The explicit callback lifetime selected by the binder input
//! (`specs/blocks/compiler.md` §111 rule 1).
//!
//! The selected aggregate gains one directive line and no other byte of
//! the mirror changes. The expected text here comes from the committed
//! mirror, never from the generator's own expression.

use std::fs;
use std::path::PathBuf;

use subscript_bindgen::{generate_for_header, generate_with_options, BindOptions};

const DIRECTIVE: &str = "// @subscript-c-callback-lifetime aggregate=\"SubCallbackInfo\"";
const CALLBACK_RECORD: &str = "// @subscript-c-callback typedef=\"SubLogCallback\"\n";
/// The aggregate the committed mirror carries, and its directive line.
/// `SubRequestInfo` is declared after `SubCallbackInfo` in the header, so
/// its record follows in the provenance block.
const COMMITTED_AGGREGATE: &str = "SubRequestInfo";
const COMMITTED_DIRECTIVE: &str =
    "// @subscript-c-callback-lifetime aggregate=\"SubRequestInfo\"\n";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn header() -> String {
    fs::read_to_string(repo().join("corpus/interop/interop.h")).expect("read interop.h")
}

fn committed_mirror() -> String {
    fs::read_to_string(repo().join("corpus/interop/interop.generated.d.ts"))
        .expect("read committed mirror")
}

/// The committed mirror without the directive its own selection adds.
fn committed_mirror_without_its_directive() -> String {
    let committed = committed_mirror();
    assert!(
        committed.contains(COMMITTED_DIRECTIVE),
        "the committed mirror carries its own selection"
    );
    committed.replacen(COMMITTED_DIRECTIVE, "", 1)
}

/// Builds the wanted text from the committed mirror: one more directive
/// line follows the record of the callback typedef that the aggregate
/// carries, which is the position of the aggregate's C struct.
fn committed_mirror_with_the_directive() -> String {
    let committed = committed_mirror();
    let at = committed
        .find(CALLBACK_RECORD)
        .expect("the committed mirror carries the callback record")
        + CALLBACK_RECORD.len();
    let mut expected = String::with_capacity(committed.len() + DIRECTIVE.len() + 1);
    expected.push_str(&committed[..at]);
    expected.push_str(DIRECTIVE);
    expected.push('\n');
    expected.push_str(&committed[at..]);
    expected
}

#[test]
fn the_selected_aggregate_adds_exactly_one_directive_line() {
    let options = BindOptions::new()
        .with_explicit_callback_lifetime(COMMITTED_AGGREGATE)
        .with_explicit_callback_lifetime("SubCallbackInfo");
    let generated =
        generate_with_options(&header(), "interop.h", &options).expect("generate the mirror");
    assert_eq!(generated, committed_mirror_with_the_directive());
}

#[test]
fn the_directive_is_the_only_added_line() {
    let options = BindOptions::new()
        .with_explicit_callback_lifetime(COMMITTED_AGGREGATE)
        .with_explicit_callback_lifetime("SubCallbackInfo");
    let generated =
        generate_with_options(&header(), "interop.h", &options).expect("generate the mirror");
    let committed = committed_mirror();
    // §111 rule 1: the records take the header's declaration order, not
    // the option order, and this selection is given in the other order.
    let records: Vec<&str> = generated
        .lines()
        .filter(|line| line.starts_with("// @subscript-c-callback-lifetime"))
        .collect();
    assert_eq!(
        records,
        vec![DIRECTIVE, COMMITTED_DIRECTIVE.trim_end_matches('\n')]
    );
    assert_eq!(generated.lines().count(), committed.lines().count() + 1);
}

#[test]
fn the_default_options_select_nothing() {
    let generated = generate_with_options(&header(), "interop.h", &BindOptions::new())
        .expect("generate the mirror");
    assert!(!generated.contains("// @subscript-c-callback-lifetime"));
    // A header alone produces the committed mirror without the one line
    // the committed selection adds.
    assert_eq!(generated, committed_mirror_without_its_directive());
    assert_eq!(
        generated,
        generate_for_header(&header(), "interop.h").expect("generate the mirror")
    );
}

#[test]
fn an_aggregate_the_header_does_not_declare_is_rejected() {
    let options = BindOptions::new().with_explicit_callback_lifetime("SubAbsentAggregate");
    let error = generate_with_options(&header(), "interop.h", &options)
        .expect_err("an absent aggregate is a bind error");
    assert!(
        error.to_string().contains("SubAbsentAggregate")
            && error.to_string().contains("no such struct"),
        "{error}"
    );
}

#[test]
fn an_aggregate_without_a_callback_field_is_rejected() {
    let options = BindOptions::new().with_explicit_callback_lifetime("SubTransform");
    let error = generate_with_options(&header(), "interop.h", &options)
        .expect_err("an aggregate without a callback field is a bind error");
    assert!(
        error.to_string().contains("SubTransform")
            && error.to_string().contains("no callback field"),
        "{error}"
    );
}

#[test]
fn an_absorbed_aggregate_is_rejected() {
    let options = BindOptions::new().with_explicit_callback_lifetime("SubStringView");
    let error = generate_with_options(&header(), "interop.h", &options)
        .expect_err("an absorbed aggregate is a bind error");
    assert!(
        error.to_string().contains("SubStringView") && error.to_string().contains("absorbs"),
        "{error}"
    );
}

#[test]
fn one_aggregate_selected_two_times_is_rejected() {
    let options = BindOptions::new()
        .with_explicit_callback_lifetime("SubCallbackInfo")
        .with_explicit_callback_lifetime("SubCallbackInfo");
    let error = generate_with_options(&header(), "interop.h", &options)
        .expect_err("a repeated selection is a bind error");
    assert!(
        error.to_string().contains("duplicate") && error.to_string().contains("SubCallbackInfo"),
        "{error}"
    );
}

/// A header with no foreign function, no `@subscript-external` and no
/// wire-mapped enum reaches the provenance emitter with nothing to
/// record. A selected aggregate must not fall through that gate and
/// lose its directive, so the bind fails loud instead.
#[test]
fn a_selection_in_a_header_with_no_provenance_source_is_rejected() {
    const HEADER: &str = "\
#ifndef PROBE_H
#define PROBE_H
#include <stddef.h>
typedef struct SubProbeView {
    const char *data;
    size_t len;
} SubProbeView;
typedef void (*SubProbeCallback)(SubProbeView message, void *userdata1, void *userdata2);
typedef struct SubProbeInfo {
    SubProbeCallback callback;
    void *userdata1;
    void *userdata2;
} SubProbeInfo;
#endif
";
    let options = BindOptions::new().with_explicit_callback_lifetime("SubProbeInfo");
    let error = generate_with_options(HEADER, "probe.h", &options)
        .expect_err("a function-free header carries no callback provenance");
    assert!(
        error.to_string().contains("SubProbeInfo")
            && error.to_string().contains("declares no foreign function"),
        "{error}"
    );

    // The firing control: the same header with one foreign function
    // binds, and the mirror carries the directive.
    let with_function = HEADER.replace(
        "#endif\n",
        "void subProbeStart(SubProbeInfo info);\n#endif\n",
    );
    let generated =
        generate_with_options(&with_function, "probe.h", &options).expect("generate the mirror");
    assert!(
        generated.contains("// @subscript-c-callback-lifetime aggregate=\"SubProbeInfo\""),
        "{generated}"
    );
}
