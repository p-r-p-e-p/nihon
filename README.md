<p align="center">
  <img src="assets/SubBanner.png" alt="Nihon — a Japanese-script systems programming language" width="720">
</p>

<h1 align="center">Nihon</h1>

<p align="center">
  A Rust-inspired systems programming language written with Japanese scripts.
</p>

<p align="center">
  <img src="assets/Hon.png" alt="Hon, the Nihon language mascot" width="180">
  <br>
  <em>Meet Hon, the mascot of Nihon.</em>
</p>

<p align="center">
  <a href="#current-status"><img src="https://img.shields.io/badge/status-early%20development-orange" alt="Early development"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="MIT or Apache-2.0 license"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/implementation-Rust-000000?logo=rust" alt="Implemented in Rust"></a>
</p>

## Overview

**Nihon** explores what a statically typed, systems-oriented programming language might look like when its source code is written in Japanese. Its design takes inspiration from Rust, while using kanji, hiragana, and katakana for language keywords and types.

The compiler, `nihonc`, is strict like `rustc`: it refuses sloppy code and explains how to fix it. Messages are in English, with the Japanese words always shown next to their romaji reading and meaning. `nihonc` turns a program into C and uses the C compiler of your system, so there is no virtual machine and the result is a native executable.

The project is in early development. The compiler is written in Rust, with no external dependencies. It currently includes a lexer, parser, diagnostics, name/type/mutability checks, a C code generator, the text type, and structs with methods. The grammar and language design are still evolving; the compiler does not yet implement the full planned language.

## Current status

- **Compiler:** stage 0 plus the first part of stage 1. It parses programs, checks names, types and mutability, and generates C.
- **Supported subset:** functions, constants, bindings, control flow, structs with methods (`構造体`, `実装`), references `&` and `&可変` (without a borrow checker yet), the types `整32`, `非負32`, `実64`, `真偽`, `文字` and `文字列` (text), and the macros `表示!`, `出力!`, `異常終了!`, `表明!`, `整形!` and `未実装!`. See [`nihonc/README.md`](nihonc/README.md) for the details and current limitations.
- **Not yet supported:** enums, `照合` (pattern matching), `任意` and `結果` (optional values and results), arrays, and tuples. These are the rest of stage 1.
- **Editor support:** `--message-format=json` already prints diagnostics for editors; the VS Code plugin is planned.
- **Syntax and grammar:** experimental and subject to change.
- **Long-term goal:** grow the compiler stage by stage, and finish with a self-hosted compiler written in Nihon.

## Language design

Nihon aims to combine familiar systems-language concepts with Japanese-script source code. The planned design draws inspiration from Rust concepts such as structs, enums, traits, pattern matching, and static types. These are design goals, not all features of the current compiler.

Keywords can be written in Japanese or in ASCII romaji. Romaji spellings are practical to type on a standard keyboard, but by default the compiler warns about them and suggests the Japanese form. `--romaji` turns that warning off, and `--strict` turns every warning into an error. The two together mean "clean code, romaji allowed". Romaji spellings of keywords are always reserved, so `naka` can never be used as a name.

## Example

This example uses structs and methods, which the compiler supports now. The comments give the romaji and the meaning of each Japanese word.

```nihon
構造体 点 {                       // kouzoutai ten = struct Point
    x: 整32,                      // sei32 = i32
    y: 整32,
}

実装 点 {                         // jissou ten = impl Point
    公開 関数 原点() -> 点 {      // koukai kansuu genten() -> ten = pub fn origin() -> Point
        点 { x: 0, y: 0 }
    }
}

関数 主() {                       // kansuu shu() = fn main()
    束縛 p = 点::原点();          // sokubaku p = ten::genten() = let p = Point::origin();
    もし p.x == 0 {               // moshi p.x == 0 = if p.x == 0
        表示!("原点です");        // hyouji!("genten desu") = println!("it is the origin")
    }
}
```

A program that works with the current compiler:

```nihon
関数 主() {                              // kansuu shu() = fn main()
    束縛 名前 = "世界";                  // sokubaku namae = "sekai" = let name = "world"
    表示!("こんにちは, {}!", 名前);      // hyouji!("konnichiwa, {}!", namae) = println!("hello, {}!", name)
    各 i 中 1..=3 {                      // kaku i naka 1..=3 = for i in 1..=3
        表示!("{}", i * i);              // hyouji!("{}", i * i) = println!("{}", i * i)
    }
}
```

### Romaji spelling

The structs example again, using ASCII spellings (compile it with `--romaji`). Long vowels are written by doubling the vowel, as in `kansuu` and `koukai`.

```nihon
kouzoutai ten {
    x: sei32,
    y: sei32,
}

jissou ten {
    koukai kansuu genten() -> ten {
        ten { x: 0, y: 0 }
    }
}

kansuu shu() {
    sokubaku p = ten::genten();
    moshi p.x == 0 {
        hyouji!("genten desu");
    }
}
```

## Keyword reference

| Rust concept | Japanese | Romaji | Meaning |
|---|---|---|---|
| `fn` | 関数 | `kansuu` | Function |
| `main` | 主 | `shu` | Main (the entry point) |
| `let` | 束縛 | `sokubaku` | Binding |
| `mut` | 可変 | `kahen` | Mutable |
| `const` | 定数 | `teisuu` | Constant |
| `if` / `else if` / `else` | もし / 他もし / 他 | `moshi` / `hokamoshi` / `hoka` | Conditional |
| `while` | 間 | `aida` | While |
| `loop` | 無限 | `mugen` | Endless loop |
| `for … in` | 各 x 中 | `kaku x naka` | Iteration |
| `break` / `continue` | 中断 / 続行 | `chuudan` / `zokkou` | Stop / go on |
| `return` | 返却 | `henkyaku` | Return |
| `as` | として | `toshite` | Conversion |
| `true` / `false` | 真 / 偽 | `shin` / `gi` | True / false |
| `match` | 照合 | `shougou` | Pattern matching (planned) |
| `struct` | 構造体 | `kouzoutai` | Structure |
| `impl` | 実装 | `jissou` | Implementation |
| `self` / `Self` | 自分 / 自身 | `jibun` / `jishin` | Oneself / the type itself |
| `pub` | 公開 | `koukai` | Public (accepted, ignored for now) |
| `&` / `&mut` | & / &可変 | `&` / `&kahen` | Reference / mutable reference |
| `println!` | 表示! | `hyouji!` | Display output with a new line |
| `print!` | 出力! | `shutsuryoku!` | Output without a new line |

Types: `整32` (`sei32`, 32-bit integer), `非負32` (`hifu32`, unsigned 32-bit integer), `実64` (`jitsu64`, 64-bit decimal), `真偽` (`shingi`, boolean), `文字` (`moji`, one character), `文字列` (`mojiretsu`, text).

## Getting started

### Requirements

- [Rust and Cargo](https://www.rust-lang.org/tools/install)
- A C compiler (`cc`, `gcc`, or `clang`) to build generated programs. `--check` and `--emit-c` work without one.

### Build and test

From the repository root, build and test the compiler:

```bash
cargo build --manifest-path nihonc/Cargo.toml --release
cargo test --manifest-path nihonc/Cargo.toml
```

Run the compiler through Cargo from the repository root; this avoids relying on a prebuilt binary or its location:

```bash
# Check a program without generating an executable
cargo run --manifest-path nihonc/Cargo.toml -- --check path/to/file.nihon

# Print the parsed syntax tree or generated C
cargo run --manifest-path nihonc/Cargo.toml -- --ast path/to/file.nihon
cargo run --manifest-path nihonc/Cargo.toml -- --emit-c path/to/file.nihon

# Generate and build an executable (it is created next to the source file)
cargo run --manifest-path nihonc/Cargo.toml -- path/to/file.nihon
```

For compiler-specific details, supported features, limitations, and tests, see [`nihonc/README.md`](nihonc/README.md).

## Roadmap

- [x] Keyword table and lexer
- [x] Parser and syntax diagnostics
- [x] C backend
- [x] Name, type, and mutability checks, and constants evaluated at compile time
- [x] Text type `文字列`, with `整形!`
- [x] `--message-format=json` for editors
- [x] Structs and methods (`構造体`, `実装`), references `&` and `&可変`
- [ ] Enums, `照合`, `任意` and `結果`
- [ ] Arrays and tuples
- [ ] Editor support: TextMate grammar and VS Code plugin
- [ ] Installer: release binaries, install script, and distribution packages
- [ ] Project tool (like Cargo) and a minimal standard library
- [ ] Owned text and references checked by the compiler
- [ ] Specify and stabilize the grammar
- [ ] Rewrite the compiler in Nihon, and bootstrap a compiler that can compile itself

## Contributing

Nihon is an experimental project, and feedback is welcome. Open an issue to discuss bugs, ideas, or Japanese and romaji keyword choices. Please keep in mind that language syntax and project plans may change as development progresses.

## License

Nihon is dual-licensed under the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
