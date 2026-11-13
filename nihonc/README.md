# nihonc

The compiler for **Nihon** (`.nihon`), a Rust-style language written in Japanese.

Status: stage 0 plus the first part of stage 1 (lexer, parser, checks, C code generator, the text type `文字列`, structs with methods, and enums with `照合`). No external dependencies.

## How it works

`nihonc` checks your program, turns it into C, and calls the C compiler of your system to build a native executable. There is no virtual machine. Error messages are in English, in the style of `rustc`, with a suggested fix whenever possible.

## Example

`//` marks the romaji reading and the meaning, for readers who do not know Japanese:

```nihon
関数 主() {                      // kansuu shu() = function main()
    表示!("こんにちは世界");     // hyouji!("konnichiwa sekai") = println!("hello, world")
}
```

## Build and test

From the repository root:

```bash
cargo build --manifest-path nihonc/Cargo.toml --release
cargo test --manifest-path nihonc/Cargo.toml
```

Or from this directory (`nihonc/`):

```bash
cargo build --release
cargo test        # unit, diagnostic, JSON and end-to-end tests
```

The end-to-end tests need a C compiler (`cc`, `gcc` or `clang`). Without one they are skipped, not failed.

## Usage

```bash
nihonc file.nihon                    # builds the executable next to the source file
nihonc -o out file.nihon             # builds the executable at `out`
nihonc --check file.nihon            # checks only, generates nothing
nihonc --emit-c file.nihon           # prints the generated C
nihonc --ast file.nihon              # prints the syntax tree
nihonc --tokens file.nihon           # prints the tokens
nihonc --romaji file.nihon           # accepts keywords in latin letters without warnings
nihonc --strict file.nihon           # turns every warning into an error
nihonc --message-format=json file.nihon   # one JSON diagnostic per line, for editors
```

Without a built binary, use `cargo run`:

```bash
cargo run --manifest-path nihonc/Cargo.toml -- --check file.nihon
```

The binary is `nihonc/target/release/nihonc` from the repository root, or `target/release/nihonc` from this directory.

A C compiler (`cc`, `gcc` or `clang`) is needed to build executables; set the `CC` environment variable to choose another one. `--check`, `--ast`, `--tokens` and `--emit-c` do not need one.

Exit codes: `0` success, `1` the program has errors, `2` wrong command line.

### JSON diagnostics

`--message-format=json` prints one JSON object per line on stderr, one per diagnostic, with no summary line. It is what the editor plugin reads. Each object has `level`, `code`, `message`, `file`, `spans`, `notes` and `helps`. Positions are 1-based `line` and `col` (counted in characters) plus `byte_start` and `byte_end`. A help may carry an `edit` with a `replacement`; `safe: true` means it can be applied without asking, `safe: false` means it is only a suggestion.

## The language so far

| Feature | Notes |
|---|---|
| Functions, constants, `束縛` (sokubaku, `let`), `可変` (kahen, `mut`) | the last expression of a block is its value |
| `もし` / `他もし` / `他` (moshi / hokamoshi / hoka, `if` / `else if` / `else`) | usable as expressions |
| `間`, `無限`, `各 i 中 a..b` (aida, mugen, kaku i naka: `while`, `loop`, `for i in a..b`) | `..` and `..=` ranges |
| `中断`, `続行`, `返却` (chuudan, zokkou, henkyaku: `break`, `continue`, `return`) | |
| `として` (toshite, `as`) | casts between numbers (wrapping for integers, saturating from decimals, like `as` in Rust), `真偽` and `文字` to integers, and `非負8` to `文字` |
| Macros `表示!` `出力!` `異常終了!` `表明!` `整形!` `未実装!` | `{}` placeholders and `{{` `}}` |
| Number types `整8` `整16` `整32` `整64`, `非負8` `非負16` `非負32` `非負64`, `大きさ` (ookisa, `usize`), `実32` `実64` | integers panic on overflow (exit code 101), like Rust in debug mode; `大きさ` is 64 bits for now |
| Types `真偽` `文字` `文字列` | boolean, one character, text |
| Text: `+`, `+=`, `==`, `<`, `.長さ()`, `.バイト数()` | `長さ` (nagasa) counts characters, `バイト数` (baitosuu) counts bytes; both return a `大きさ` (ookisa, `usize`) |
| Structs: `構造体 点 { x: 整32 }` (kouzoutai, `struct`), literals `点 { x: 1 }`, fields `p.x`, `p.x = 5` | assigning needs `可変` (kahen, `mut`) on the variable |
| Methods: `実装 点 { ... }` (jissou, `impl`), `点::新()` (associated function), `p.動く()` (method) | first parameter `自分` (jibun, `self`), `&自分`, `&可変 自分`; `自身` (jishin, `Self`) is the struct's own type |
| Enums: `列挙型 形 { 円(実64), 無し }` (rekkyogata, `enum`), values `形::円(1.0)` and `形::無し` | variants may carry values of any type, even structs and other enums |
| `照合 x { 形::円(r) => ..., _ => ... }` (shougou, `match`) | an expression; patterns: `_`, names, numbers, `文字`, `文字列`, `真` / `偽`, ranges `1..=5`, alternatives `a \| b`, and variants with nested patterns. The compiler checks that every case is covered (E0640) and warns about arms that can never run (W0006) |
| References: `&p`, `&可変 p`, types `&点` and `&可変 点` | syntax and mutability are checked; there is no borrow checker yet |
| `公開` (koukai, `pub`) | accepted and ignored for now: everything is public |

The entry point is a function named `主` (shu, "main") or `shu`. Neither gets a warning.

## Project structure

- `src/source.rs`: source text, byte spans, display-column widths (kanji = 2 columns)
- `src/width_table.rs`: generated by `tools/gen_width.py` (do not edit by hand)
- `src/diag/`: diagnostics, the error code catalog, human and JSON output
- `src/lexer/`: lexer, character classes, and the Japanese/romaji keyword table
- `src/ast.rs`, `src/parser.rs`: syntax tree and parser (recursive descent plus Pratt, with error recovery)
- `src/codegen_c.rs`: name, type and mutability checks, constant evaluation, and C code generation
- `tests/ui/`: one `.nihon` input and its expected `.stderr` per case (`BLESS=1 cargo test --test ui` rewrites the expected files)
- `tests/run.rs`: end-to-end tests (compile, run, compare the output)
- `tests/json.rs`: tests for `--message-format=json`

## Known limitations

- Integer overflow and division by zero stop the program with exit code 101, like Rust in debug mode.
- Constants are computed while compiling (numbers, `真偽`, `文字`, `文字列`, other constants, operators, `として`, `もし` / `他`). A calculation that overflows or divides by zero is error E0208. Function calls and macros are not allowed in constants.
- `真偽` values print as `真 (shin)` / `偽 (gi)`.
- Text built while the program runs (with `整形!` or `+`) is never freed, because there is no owned text type yet.
- Structs behave like plain values: passing one copies it, and there is no move checking yet.
- References point only to structs, and can be used only as parameters and local variables. A function cannot return a reference and a field cannot hold one (error E0622), because there is no borrow checker to keep them safe.
- Structs cannot be printed with `表示!` yet (error E0629) and cannot be compared with `==`: compare the fields one by one. Constants cannot have a struct type.
- A struct literal is not allowed directly in the header of `もし`, `間` or `各` (like in Rust); wrap it in parentheses.
- Enums are plain values too (copied when passed around). They cannot have methods yet (`実装` works on structs only), cannot be compared with `==`, and cannot be printed: examine them with `照合`.
- In `照合`, a `|` pattern cannot bind names (use one arm per case), there are no guards (`もし` after a pattern) and no struct or tuple patterns yet. Integer ranges that cover a whole type still need a `_` arm.
- Not supported yet: `任意`, `結果`, arrays and tuples. They are reserved words, and using them gives a clear error.
