//! O `--message-format=json` entrega erros e correcoes que o editor consegue ler.

use std::process::Command;

fn run_json(name: &str, src: &str) -> (String, i32) {
    let dir = std::env::temp_dir().join(format!("nihonc-json-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{}.nihon", name));
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_nihonc"))
        .arg("--check")
        .arg("--message-format=json")
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (String::from_utf8(out.stderr).unwrap(), out.status.code().unwrap_or(-1))
}

#[test]
fn json_has_one_object_per_line_and_no_summary() {
    let (err, code) = run_json("zen", "関数 主() {\n    束縛 x = １;\n}\n");
    assert_eq!(code, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "{}", err);
    assert!(lines[0].starts_with("{\"level\":\"error\",\"code\":\"E0002\""), "{}", err);
    assert!(lines[0].contains("\"replacement\":\"1\",\"safe\":true"), "{}", err);
}

#[test]
fn json_for_a_clean_program_prints_nothing() {
    let (err, code) = run_json("ok", "関数 主() { 表示!(\"oi\"); }\n");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(err, "");
}

#[test]
fn unknown_format_is_rejected() {
    let out = Command::new(env!("CARGO_BIN_EXE_nihonc"))
        .arg("--message-format=xml")
        .arg("x.nihon")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown message format"));
}
