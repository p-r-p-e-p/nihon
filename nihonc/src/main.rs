//! nihonc: compilador da linguagem Nihon (stage 0, fatia 1: lexer + erros).

mod ast;
mod codegen_c;
mod diag;
mod lexer;
mod parser;
mod source;
mod width_table;

use diag::json::to_json;
use diag::render::render;
use diag::{Config, Handler};
use lexer::{lex, Token};
use std::process::Command;
use source::SourceFile;
use std::io::IsTerminal;

/// Como os diagnosticos sao impressos no stderr.
#[derive(Clone, Copy)]
enum Style {
    Human { color: bool },
    Json,
}

const USAGE: &str = "\
nihonc - compiler for the Nihon language (stage 0, slice 2: parser + C backend)

USAGE:
    nihonc [OPTIONS] <file.nihon>

OPTIONS:
    --strict       turn every warning into an error
    --romaji       accept keywords written in latin letters without warnings
    --tokens       print the tokens to stdout
    --ast          print the syntax tree to stdout (no code generation)
    --check        check the program, but do not generate anything
    --message-format=<fmt>
                   `human` (default) or `json`: one JSON object per line on
                   stderr for each diagnostic, for editors and tools
    --emit-c       print the generated C to stdout instead of building
    -o <file>      name of the executable (default: next to the source file, without `.nihon`)
    -h, --help     show this help
    -V, --version  show the version
";

struct Args {
    config: Config,
    tokens: bool,
    ast: bool,
    check: bool,
    emit_c: bool,
    json: bool,
    out: Option<String>,
    path: String,
}

fn parse_args() -> Result<Args, String> {
    let mut config = Config::default();
    let mut tokens = false;
    let (mut ast, mut check, mut emit_c) = (false, false, false);
    let mut json = false;
    let mut out: Option<String> = None;
    let mut path: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ast" => ast = true,
            "--check" => check = true,
            "--emit-c" => emit_c = true,
            "-o" => match it.next() {
                Some(v) => out = Some(v),
                None => return Err("`-o` needs a file name".to_string()),
            },
            "--strict" => config.strict = true,
            "--romaji" => config.romaji = true,
            "--tokens" => tokens = true,
            "-h" | "--help" => {
                print!("{}", USAGE);
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("nihonc {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            s if s.starts_with("--message-format=") => {
                match &s["--message-format=".len()..] {
                    "json" => json = true,
                    "human" => json = false,
                    other => {
                        return Err(format!(
                            "unknown message format `{}` (use `human` or `json`)",
                            other
                        ))
                    }
                }
            }
            s if s.starts_with('-') => return Err(format!("unknown option `{}`", s)),
            s => {
                if path.is_some() {
                    return Err("only one input file is supported".to_string());
                }
                path = Some(s.to_string());
            }
        }
    }
    match path {
        Some(path) => Ok(Args {
            config,
            tokens,
            ast,
            check,
            emit_c,
            json,
            out,
            path,
        }),
        None => Err("no input file".to_string()),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

fn dump_tokens(file: &SourceFile, tokens: &[Token]) {
    for t in tokens {
        let (line, col) = file.line_col(t.span.start);
        println!(
            "{}:{}\t{:?}\t{:?}",
            line,
            col,
            t.kind,
            &file.text[t.span.range()]
        );
    }
}

/// Mostra os diagnosticos ainda nao mostrados; devolve quantos ja foram.
fn print_new(handler: &Handler, file: &SourceFile, style: Style, shown: usize) -> usize {
    for d in &handler.diags[shown..] {
        match style {
            Style::Human { color } => eprintln!("{}", render(d, file, color)),
            Style::Json => eprintln!("{}", to_json(d, file)),
        }
    }
    handler.diags.len()
}

fn finish_errors(handler: &Handler, file: &SourceFile, style: Style, shown: usize) -> i32 {
    print_new(handler, file, style, shown);
    if let Style::Json = style {
        // em JSON so os diagnosticos saem, sem a linha de resumo
        return 1;
    }
    let errors = handler.error_count();
    let warnings = handler.warning_count();
    let mut line = format!(
        "error: aborting due to {}",
        plural(errors, "previous error", "previous errors")
    );
    if warnings > 0 {
        line.push_str(&format!("; {} emitted", plural(warnings, "warning", "warnings")));
    }
    eprintln!("{}", line);
    1
}

fn summary_warnings(handler: &Handler, style: Style) {
    if let Style::Json = style {
        return;
    }
    let w = handler.warning_count();
    if w > 0 {
        eprintln!("warning: {} emitted", plural(w, "warning", "warnings"));
    }
}

const NO_CC: &str = "error: no C compiler found

nihonc turns your program into C and uses the C compiler of your system to
build the executable. Install one of these and try again:
  Arch Linux:     sudo pacman -S gcc
  Debian/Ubuntu:  sudo apt install gcc
  Fedora:         sudo dnf install gcc
  macOS:          xcode-select --install
  Windows:        install MSYS2 (gcc) or LLVM (clang) and put it in the PATH
You can also point to your compiler with the CC environment variable.
To only see the C code, run again with `--emit-c`.";

fn find_cc() -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(cc) = std::env::var_os("CC") {
        candidates.push(cc.to_string_lossy().to_string());
    }
    candidates.extend(["cc", "gcc", "clang"].map(String::from));
    candidates.into_iter().find(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn build(args: &Args, c_code: &str) -> i32 {
    let Some(cc) = find_cc() else {
        eprintln!("{}", NO_CC);
        return 1;
    };
    let exe = match &args.out {
        Some(o) => o.clone(),
        None => {
            // por padrao, ao lado do codigo fonte
            let src = std::path::Path::new(&args.path);
            let stem = src
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "a".to_string());
            let name = if cfg!(windows) {
                format!("{}.exe", stem)
            } else {
                stem
            };
            match src.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => dir.join(name).to_string_lossy().to_string(),
                _ => name,
            }
        }
    };
    let dir = std::env::temp_dir().join(format!("nihonc-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("error: cannot create a temporary folder: {}", e);
        return 1;
    }
    let c_path = dir.join("out.c");
    if let Err(e) = std::fs::write(&c_path, c_code) {
        eprintln!("error: cannot write the generated C: {}", e);
        return 1;
    }
    let out = Command::new(&cc)
        .arg("-O1")
        .arg("-o")
        .arg(&exe)
        .arg(&c_path)
        .arg("-lm")
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let _ = std::fs::remove_dir_all(&dir);
            0
        }
        Ok(o) => {
            eprintln!(
                "error: internal compiler error: the C compiler `{}` rejected the generated code",
                cc
            );
            eprintln!("{}", String::from_utf8_lossy(&o.stderr));
            eprintln!("note: this is a bug in nihonc; the generated C was kept at {}", c_path.display());
            1
        }
        Err(e) => {
            eprintln!("error: cannot run `{}`: {}", cc, e);
            1
        }
    }
}

fn run() -> i32 {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {}\n\n{}", msg, USAGE);
            return 2;
        }
    };

    let text = match std::fs::read_to_string(&args.path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            eprintln!("error: `{}` is not valid UTF-8", args.path);
            return 2;
        }
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", args.path, e);
            return 2;
        }
    };
    let text = text.strip_prefix('\u{FEFF}').map(str::to_string).unwrap_or(text);

    let file = SourceFile::new(args.path.clone(), text);
    let mut handler = Handler::new(args.config);
    let tokens = lex(&file, &mut handler);

    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let style = if args.json {
        Style::Json
    } else {
        Style::Human { color }
    };
    let shown = 0;
    if handler.error_count() > 0 {
        return finish_errors(&handler, &file, style, shown);
    }
    let shown = print_new(&handler, &file, style, shown);

    if args.tokens {
        dump_tokens(&file, &tokens);
        return 0;
    }

    let Some(program) = parser::parse(&file, &tokens, &mut handler) else {
        return finish_errors(&handler, &file, style, shown);
    };
    if args.ast {
        print_new(&handler, &file, style, shown);
        println!("{:#?}", program);
        return 0;
    }

    let Some(c_code) = codegen_c::generate(&file, &program, &mut handler) else {
        return finish_errors(&handler, &file, style, shown);
    };
    print_new(&handler, &file, style, shown);
    summary_warnings(&handler, style);

    if args.check {
        return 0;
    }
    if args.emit_c {
        print!("{}", c_code);
        return 0;
    }
    build(&args, &c_code)
}

fn main() {
    std::process::exit(run());
}
