//! Testes de ponta a ponta: compila programas Nihon com o `nihonc`, roda o
//! executavel e compara a saida. Precisam de um compilador de C no sistema.

use std::path::PathBuf;
use std::process::Command;

fn has_cc() -> bool {
    ["cc", "gcc", "clang"].iter().any(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nihonc-e2e-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compila e roda; devolve (stdout, stderr, codigo de saida).
fn run(name: &str, src: &str) -> (String, String, i32) {
    let dir = workdir(name);
    let file = dir.join(format!("{}.nihon", name));
    std::fs::write(&file, src).unwrap();
    let exe = dir.join("prog");
    let out = Command::new(env!("CARGO_BIN_EXE_nihonc"))
        .arg("--romaji")
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}: nihonc falhou:\n{}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );
    let r = Command::new(&exe).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (
        String::from_utf8(r.stdout).unwrap(),
        String::from_utf8(r.stderr).unwrap(),
        r.status.code().unwrap_or(-1),
    )
}

fn ok(name: &str, src: &str, expected: &str) {
    if !has_cc() {
        eprintln!("sem compilador de C: pulando {}", name);
        return;
    }
    let (out, err, code) = run(name, src);
    assert_eq!(code, 0, "{}: codigo de saida; stderr:\n{}", name, err);
    assert_eq!(out, expected, "{}", name);
}

#[test]
fn hello() {
    ok("hello", "関数 主() { 表示!(\"こんにちは世界\"); }", "こんにちは世界\n");
}

#[test]
fn romaji_program() {
    ok("romaji", "kansuu shu() { hyouji!(\"oi {}\", 1 + 2); }", "oi 3\n");
}

#[test]
fn factorial_and_fizzbuzz() {
    let src = include_str!("ui/fatorial_fizzbuzz.nihon");
    ok(
        "fizz",
        src,
        "1\n2\nFizz\n24\nBuzz\nFizz\n5040\n40320\nFizz\nBuzz\n39916800\nFizz\n",
    );
}

#[test]
fn floats_print_like_rust() {
    let src = r#"
関数 主() {
    表示!("{} {} {} {}", 1.0, 0.1 + 0.2, 1.5e10, 0.000001);
    表示!("{} {}", 100.0 / 3.0, 2.0 * 0.5);
    束縛 n: 整32 = 7;
    表示!("{} {}", n として 実64, 3.99 として 整32);
}
"#;
    ok("floats", src, "1 0.30000000000000004 15000000000 0.000001\n33.333333333333336 1\n7 3\n");
}

#[test]
fn shadowing_loops_and_blocks() {
    let src = r#"
関数 主() {
    束縛 x = 1;
    束縛 x = x + 1;
    束縛 可変 soma = 0;
    各 i 中 0..10 {
        もし i % 2 == 0 { 続行; }
        もし i > 7 { 中断; }
        soma += i;
    }
    束縛 y = もし x == 2 { 10 } 他 { 20 };
    束縛 可変 k = 0;
    無限 {
        k += 1;
        もし k == 5 { 中断; }
    }
    表示!("{} {} {} {}", x, soma, y, k);
}
"#;
    ok("shadow", src, "2 16 10 5\n");
}

#[test]
fn inclusive_range_at_the_edge_of_the_type() {
    let src = r#"
関数 主() {
    束縛 可変 n = 0;
    各 i 中 2147483645..=2147483647 { n += 1; }
    各 j 中 5..=4 { n += 100; }
    表示!("{}", n);
}
"#;
    ok("edge", src, "3\n");
}

#[test]
fn short_circuit_runs_calls_only_when_needed() {
    let src = r#"
関数 ruido(x: 整32) -> 真偽 {
    表示!("ruido {}", x);
    真
}
関数 主() {
    束縛 a = 偽 && ruido(1);
    束縛 b = 真 || ruido(2);
    束縛 c = 真 && ruido(3);
    表示!("{} {} {}", a, b, c);
}
"#;
    ok("short", src, "ruido 3\n偽 (gi) 真 (shin) 真 (shin)\n");
}

#[test]
fn evaluation_order_is_left_to_right() {
    let src = r#"
関数 f(x: 整32) -> 整32 { 表示!("f{}", x); x }
関数 主() {
    表示!("{}", f(1) + f(2) * f(3));
}
"#;
    ok("order", src, "f1\nf2\nf3\n7\n");
}

#[test]
fn constants_and_chars() {
    let src = r#"
定数 B: 整32 = A * 2;
定数 A: 整32 = 21;
関数 主() {
    表示!("{} {} {}", A, B, 'あ');
    出力!("a");
    出力!("b\n");
}
"#;
    ok("consts", src, "21 42 あ\nab\n");
}

fn fails(name: &str, src: &str, needle: &str) {
    if !has_cc() {
        return;
    }
    let (out, err, code) = run(name, src);
    assert_eq!(code, 101, "{}: stdout={:?} stderr={:?}", name, out, err);
    assert!(err.contains(needle), "{}: stderr={:?}", name, err);
}

#[test]
fn panics_exit_with_101() {
    fails(
        "overflow",
        "関数 主() { 束縛 可変 x: 整32 = 2147483647; x += 1; 表示!(\"{}\", x); }",
        "attempt to add with overflow",
    );
    fails(
        "div0",
        "関数 f(d: 整32) -> 整32 { 10 / d } 関数 主() { 表示!(\"{}\", f(0)); }",
        "attempt to divide by zero",
    );
    fails("panic", "関数 主() { 異常終了!(\"deu ruim {}\", 42); }", "deu ruim 42");
    fails("assert", "関数 主() { 表明!(1 > 2, \"um {}\", 1); }", "um 1");
    fails("todo", "関数 主() { 未実装!(); }", "not yet implemented");
}

#[test]
fn stdout_is_flushed_before_a_panic() {
    if !has_cc() {
        return;
    }
    let (out, _, code) = run("flush", "関数 主() { 表示!(\"antes\"); 異常終了!(\"fim\"); }");
    assert_eq!(code, 101);
    assert_eq!(out, "antes\n");
}

#[test]
fn constants_are_folded_at_compile_time() {
    let src = r#"
定数 A: 整32 = 2 + 3 * 4;
定数 B: 実64 = A として 実64 / 4.0;
定数 C: 整32 = もし A > 10 { -A } 他 { A };
定数 D: 非負32 = 4000000000 + 1;
定数 E: 整32 = -2147483648;
関数 主() { 表示!("{} {} {} {} {}", A, B, C, D, E); }
"#;
    ok("constfold", src, "14 3.5 -14 4000000001 -2147483648\n");
}
