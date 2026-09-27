use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn checked_sticky_is_false_for_every_accepted_flag_set() {
    let mut source = String::from("export function main(): void {");
    let mut expected = String::new();
    for mask in 0..128 {
        let flags: String = b"dgimsuv"
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, flag)| char::from(*flag))
            .collect();
        if flags.contains('u') && flags.contains('v') {
            continue;
        }
        source.push_str(&format!("print(`${{/a/{flags}.sticky}}`);"));
        expected.push_str("false\n");
    }
    source.push('}');
    let checked = check_program(&[SourceFile::new("test.ts", source)]).expect("accepted flag sets");
    let lir = lower_module(&checked).expect("lower sticky accessors");
    assert_eq!(interpret(&lir).expect("sticky values"), expected.as_bytes());
}
