//! Testes de interface: roda o `nihonc` em cada `tests/ui/*.nihon` e compara
//! o stderr com o `.stderr` ao lado. Flags opcionais ficam em `.flags`.
//! Para gravar/atualizar as saidas esperadas: `BLESS=1 cargo test --test ui`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn ui() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui");
    let bless = std::env::var_os("BLESS").is_some();

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "nihon"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "nenhum caso em tests/ui");

    let mut failures: Vec<String> = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_str().unwrap();
        let flags: Vec<String> = fs::read_to_string(path.with_extension("flags"))
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        let out = Command::new(env!("CARGO_BIN_EXE_nihonc"))
            .current_dir(&dir)
            .args(&flags)
            .arg(name)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        let expected_path = path.with_extension("stderr");

        if bless {
            fs::write(&expected_path, &stderr).unwrap();
            continue;
        }

        let expected = match fs::read_to_string(&expected_path) {
            Ok(e) => e,
            Err(_) => {
                failures.push(format!("{}: falta o .stderr (rode com BLESS=1)", name));
                continue;
            }
        };
        let want_code = if stderr.contains("aborting due to") { 1 } else { 0 };
        if out.status.code() != Some(want_code) {
            failures.push(format!("{}: codigo de saida {:?}, esperado {}", name, out.status.code(), want_code));
        }
        if stderr != expected {
            failures.push(format!(
                "{}: saida diferente\n--- esperado ---\n{}--- obtido ---\n{}",
                name, expected, stderr
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
