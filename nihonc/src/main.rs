//! nihonc: compilador da linguagem Nihon (stage 0, fatia 1: lexer + erros).

mod diag;
mod lexer;
mod source;
mod width_table;

use diag::render::render;
use diag::{Config, Handler};
use lexer::{lex, Token};
use source::SourceFile;
use std::io::IsTerminal;

const USAGE: &str = "\
nihonc - compiler for the Nihon language (stage 0, slice 1: lexer)

USAGE:
    nihonc [OPTIONS] <file.nihon>

OPTIONS:
    --strict       turn every warning into an error
    --romaji       accept keywords written in latin letters without warnings
    --tokens       print the tokens to stdout
    -h, --help     show this help
    -V, --version  show the version
";

struct Args {
    config: Config,
    tokens: bool,
    path: String,
}

fn parse_args() -> Result<Args, String> {
    let mut config = Config::default();
    let mut tokens = false;
    let mut path: Option<String> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
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
    for d in &handler.diags {
        eprintln!("{}", render(d, &file, color));
    }

    let errors = handler.error_count();
    let warnings = handler.warning_count();
    if errors > 0 {
        let mut line = format!(
            "error: aborting due to {}",
            plural(errors, "previous error", "previous errors")
        );
        if warnings > 0 {
            line.push_str(&format!("; {} emitted", plural(warnings, "warning", "warnings")));
        }
        eprintln!("{}", line);
        return 1;
    }
    if warnings > 0 {
        eprintln!("warning: {} emitted", plural(warnings, "warning", "warnings"));
    }

    if args.tokens {
        dump_tokens(&file, &tokens);
    } else {
        println!("ok: {} tokens", tokens.len() - 1);
    }
    0
}

fn main() {
    std::process::exit(run());
}
