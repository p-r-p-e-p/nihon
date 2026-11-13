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

#[test]
fn executable_goes_next_to_the_source_by_default() {
    if !has_cc() {
        return;
    }
    let dir = workdir("nextto");
    let file = dir.join("oi.nihon");
    std::fs::write(&file, "関数 主() { 表示!(\"oi\"); }").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_nihonc")).arg(&file).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let exe = dir.join(if cfg!(windows) { "oi.exe" } else { "oi" });
    assert!(exe.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_values_and_methods() {
    let src = r#"
定数 NOME: 文字列 = "世界" + "!";
関数 saudar(quem: 文字列) -> 文字列 {
    整形!("こんにちは, {}", quem)
}
関数 主() {
    束縛 可変 s: 文字列 = "あい";
    s += "う";
    表示!("{} {} {}", s, s.長さ(), s.バイト数());
    表示!("{}", saudar(NOME));
    束縛 t = もし s == "あいう" { "igual" } 他 { "diferente" };
    表示!("{} {} {}", t, "a" < "b", "abc" > "abd");
    束縛 vazio = "";
    表示!("[{}] {}", vazio, vazio.長さ());
    束縛 n = 3;
    束縛 f = 整形!("{}-{}-{}-{}", n, 1.5, 'x', 真偽_ok());
    表示!("{} {}", f, f.nagasa());
    表示!("a\tb\"c\\d");
}
関数 真偽_ok() -> 真偽 { 真 }
"#;
    ok(
        "text",
        src,
        "あいう 3 9\nこんにちは, 世界!\nigual 真 (shin) 偽 (gi)\n[] 0\n3-1.5-x-真 (shin) 16\na\tb\"c\\d\n",
    );
}

#[test]
fn format_can_be_used_in_a_loop_and_in_panics() {
    ok(
        "fmtloop",
        r#"
関数 主() {
    束縛 可変 acc = "";
    各 i 中 0..5 { acc += 整形!("{},", i); }
    表示!("{}", acc);
}
"#,
        "0,1,2,3,4,\n",
    );
    fails(
        "panictext",
        r#"関数 主() { 束縛 m = 整形!("x{}", 7); 異常終了!("{} deu ruim", m); }"#,
        "x7 deu ruim",
    );
}

#[test]
fn structs_methods_and_references() {
    let src = r#"
構造体 点 {
    x: 整32,
    y: 整32,
}

構造体 線 {
    始: 点,
    終: 点,
    名前: 文字列,
}

実装 点 {
    公開 関数 原点() -> 点 {
        点 { x: 0, y: 0 }
    }
    関数 新(x: 整32, y: 整32) -> 自身 {
        自身 { x: x, y: y }
    }
    関数 和(&自分) -> 整32 {
        自分.x + 自分.y
    }
    関数 動く(&可変 自分, dx: 整32) {
        自分.x += dx;
        自分.y = 自分.y + 1;
    }
    関数 複製(自分) -> 点 {
        自分
    }
}

関数 足す(p: &点) -> 整32 {
    p.x + p.y
}

関数 増やす(p: &可変 点) {
    p.x = p.x + 10;
    p.動く(100);
}

関数 主() {
    束縛 可変 p = 点::新(1, 2);
    p.動く(5);
    増やす(&可変 p);
    表示!("{} {} {}", p.x, p.y, p.和());
    表示!("{}", 足す(&p));
    束縛 可変 l = 線 { 始: 点::原点(), 終: p, 名前: "線" };
    l.終.x = 7;
    l.名前 += "!";
    l.始.動く(2);
    表示!("{} {} {} {}", l.終.x, l.始.x, l.始.y, l.名前);
    束縛 q = p.複製();
    p.x = 0;
    表示!("{} {}", q.x, p.x);
    もし q.x == 116 { 表示!("ok"); } 他 { 表示!("no"); }
    もし (点 { x: 1, y: 2 }).和() == 3 { 表示!("lit"); }
}
"#;
    ok("structs", src, "116 4 120\n120\n7 2 1 線!\n116 0\nok\nlit\n");
}

#[test]
fn all_number_types() {
    let src = r#"
定数 最大: 非負8 = 255;
定数 小: 整8 = -128;
定数 大: 整64 = 9223372036854775807;
定数 全部: 非負64 = 18446744073709551615;
定数 割合: 実32 = 0.1;
定数 変換: 非負8 = (300 として 整32) として 非負8;

関数 倍(x: 大きさ) -> 大きさ { x * 2 }

関数 主() {
    束縛 a: 整8 = 100;
    束縛 b: 整16 = -30000;
    束縛 c: 整64 = 5000000000;
    束縛 d: 非負8 = 200;
    束縛 e: 大きさ = 10;
    束縛 f: 実32 = 0.1;
    束縛 g: 実32 = f * 3.0;
    表示!("{} {} {} {} {}", a, b, c, d, e);
    表示!("{} {} {} {}", 最大, 小, 大, 全部);
    表示!("{} {} {}", f, g, 割合);
    表示!("{} {}", 変換, 300.7 として 非負8);
    表示!("{} {}", -5 として 非負8, 1e10 として 整32);
    表示!("{} {}", 97 として 非負8 として 文字, 3.99 として 整8);
    表示!("{}", 倍(e));
    各 i 中 0..e {
        もし i == 3 { 表示!("{}", i); }
    }
    各 j 中 1..=3 として 大きさ { 表示!("{}", j); }
    束縛 h: 実64 = f として 実64;
    表示!("{}", h);
    表示!("{} {}", 7 として 整8 / 2, -7 として 整64 % 3);
}
"#;
    ok(
        "numbers",
        src,
        "100 -30000 5000000000 200 10\n255 -128 9223372036854775807 18446744073709551615\n0.1 0.3 0.1\n44 255\n251 2147483647\na 3\n20\n3\n1\n2\n3\n0.10000000149011612\n3 -1\n",
    );
}

#[test]
fn narrow_types_panic_on_overflow() {
    fails(
        "i8_overflow",
        "関数 主() { 束縛 可変 x: 整8 = 127; x += 1; 表示!(\"{}\", x); }",
        "attempt to add with overflow",
    );
    fails(
        "u8_underflow",
        "関数 主() { 束縛 可変 x: 非負8 = 0; x -= 1; 表示!(\"{}\", x); }",
        "attempt to subtract with overflow",
    );
    fails(
        "u64_mul",
        "関数 主() { 束縛 x: 非負64 = 4294967296; 表示!(\"{}\", x * x); }",
        "attempt to multiply with overflow",
    );
    fails(
        "i64_neg",
        "関数 主() { 束縛 x: 整64 = -9223372036854775807 - 1; 表示!(\"{}\", -x); }",
        "attempt to negate with overflow",
    );
}

#[test]
fn enums_and_match() {
    let src = r#"
列挙型 色 { 赤, 緑, 青 }

列挙型 形 {
    円(実64),
    四角(実64, 実64),
    無し,
}

列挙型 結 { 良(整32), 悪(文字列), 空 }

構造体 箱 { 色: 色, 形: 形 }

関数 名前(c: 色) -> 文字列 {
    照合 c {
        色::赤 => "赤",
        色::緑 => "緑",
        色::青 => "青",
    }
}

関数 面積(f: 形) -> 実64 {
    照合 f {
        形::円(r) => r * r * 3.0,
        形::四角(w, h) => w * h,
        形::無し => 0.0,
    }
}

関数 分類(n: 整32) -> 文字列 {
    照合 n {
        0 => "zero",
        1 | 2 | 3 => "small",
        4..=9 => "medium",
        -5..0 => "neg",
        _ => "big",
    }
}

関数 値(r: 結) -> 整32 {
    照合 r {
        結::良(n) => 返却 n,
        結::悪(_) => 9,
        結::空 => 0,
    }
}

関数 次(c: 色) -> 色 {
    照合 c { 色::赤 => 色::緑, 色::緑 => 色::青, 色::青 => 色::赤 }
}

関数 主() {
    表示!("{}", 名前(色::緑));
    表示!("{} {} {}", 面積(形::円(2.0)), 面積(形::四角(2.0, 5.0)), 面積(形::無し));
    表示!("{} {} {} {} {}", 分類(0), 分類(2), 分類(7), 分類(-3), 分類(100));
    表示!("{} {} {}", 値(結::良(5)), 値(結::悪("x")), 値(結::空));
    束縛 b = 箱 { 色: 色::青, 形: 形::四角(1.5, 2.0) };
    照合 b.形 {
        形::四角(w, _) => 表示!("w={}", w),
        _ => 表示!("outra"),
    }
    束縛 x = 照合 真 { 真 => 1, 偽 => 2 };
    表示!("{}", x);
    照合 'a' { 'a'..='z' => 表示!("minuscula"), _ => 表示!("outro") }
    照合 "oi" { "oi" => 表示!("ola"), _ => 表示!("?") }
    束縛 可変 c = 色::赤;
    各 i 中 0..6 {
        c = 次(c);
        照合 c {
            色::赤 => 続行,
            色::緑 => { 表示!("g{}", i); }
            色::青 => 中断,
        }
    }
    表示!("{}", 名前(c));
}
"#;
    ok(
        "enums",
        src,
        "緑\n12 10 0\nzero small medium neg big\n5 9 0\nw=1.5\n1\nminuscula\nola\ng0\n青\n",
    );
}

#[test]
fn text_length_is_a_size() {
    let src = r#"
関数 主() {
    束縛 s = "こんにちは";
    束縛 n: 大きさ = s.長さ();
    束縛 b: 大きさ = s.バイト数();
    表示!("{} {}", n, b);
    束縛 m = s.長さ() として 整32;
    表示!("{}", m - 10);
    各 i 中 0..s.長さ() { 表示!("{}", i); }
}
"#;
    ok("text_len", src, "5 15\n-5\n0\n1\n2\n3\n4\n");
}
