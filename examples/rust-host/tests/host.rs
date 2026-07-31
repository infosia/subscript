use subscript_example_rust_host::run;

#[test]
fn embedded_flow_is_exact_and_refusal_names_the_declaration() -> Result<(), String> {
    let (stdout, stderr_lines) = run()?;

    assert_eq!(
        stdout,
        concat!(
            "tick=1, helper=2\n",
            "tick=2, helper=4\n",
            "tick=3, helper=6\n",
            "tick=4, helper=40\n",
            "tick=5, helper=50\n",
            "tick=6, helper=60\n",
        )
        .as_bytes()
    );
    assert_eq!(stderr_lines.len(), 1);
    assert!(
        stderr_lines[0].contains("function doubled"),
        "refusal did not name the declaration: {}",
        stderr_lines[0]
    );

    Ok(())
}
