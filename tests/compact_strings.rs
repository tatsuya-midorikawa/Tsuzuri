use std::process::Command;
use tsuzuri::llvm::StringForm;

#[test]
fn the_forms_have_names_and_the_default_is_utf16() {
    assert_eq!(StringForm::DEFAULT, StringForm::Utf16);
    assert_eq!(StringForm::default(), StringForm::DEFAULT);
    for form in [StringForm::Utf16, StringForm::Compact] {
        assert_eq!(StringForm::parse(form.name()), Some(form));
    }
    for name in ["", "UTF16", "Compact", "latin1", "utf16 ", "off", "1"] {
        assert_eq!(StringForm::parse(name), None, "{name:?}");
    }
}

#[test]
fn the_environment_chooses_the_form_and_a_wrong_name_is_an_error() {
    let root = std::env::temp_dir().join(format!("tsuzuri-strings-form-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), "let text = \"hello\"\ntext.length\n").unwrap();
    let build = |value: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .args(["build", "--emit", "llvm", "-O0", "-o"])
            .arg(root.join("out.ll"))
            .arg(root.join("Main.tz"))
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_STRINGS");
        if let Some(value) = value {
            command.env("TSUZURI_STRINGS", value);
        }
        let output = command.output().unwrap();
        let ir = std::fs::read_to_string(root.join("out.ll")).ok();
        let _ = std::fs::remove_file(root.join("out.ll"));
        (output, ir)
    };
    // The form belongs to the cache key: one cache directory serves every build.
    for value in [
        None,
        Some("utf16"),
        Some(""),
        Some("compact"),
        Some("utf16"),
        None,
    ] {
        let (output, ir) = build(value);
        assert!(output.status.success(), "{value:?}");
        assert!(ir.is_some(), "{value:?}");
    }
    let (output, ir) = build(Some("latin1"));
    assert_eq!(output.status.code(), Some(1));
    assert!(ir.is_none());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("E2000")
            && stderr.contains("TSUZURI_STRINGS must be utf16 or compact, not 'latin1'"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
