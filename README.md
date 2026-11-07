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
  <a href="#status"><img src="https://img.shields.io/badge/status-early%20development-orange" alt="Early development"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="MIT or Apache-2.0 license"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/implementation-Rust-000000?logo=rust" alt="Implemented in Rust"></a>
</p>

## Overview

**Nihon** explores what a statically typed, systems-oriented programming language might look like when its source code is written in Japanese. Its design takes inspiration from Rust, while using kanji, hiragana, and katakana for language keywords and types.

The project is at an early stage: the current compiler is a stage-zero implementation focused on lexical analysis and diagnostics. The language grammar and syntax are still evolving, and the compiler is not yet ready for general-purpose development.

## Current status

- **Compiler:** stage 0; lexer and diagnostics are implemented.
- **Parser and type checker:** not yet implemented.
- **Syntax:** experimental and subject to change.
- **Long-term goal:** build an initial compiler, then work toward a self-hosted compiler written in Nihon.

See the [roadmap](#roadmap) for planned milestones.

## Language design

Nihon aims to combine familiar systems-language concepts with Japanese-script source code. The planned design draws inspiration from Rust concepts such as structs, enums, traits, pattern matching, and static types. These are design goals, not all features of the current compiler.

Keywords can be written in Japanese or in ASCII romaji. Romaji spellings are designed to be practical to type on a standard keyboard.

## Example

The syntax below illustrates the language's direction. It is **provisional** and is not yet supported by the current compiler.

```nihon
構造体 点 {
    x: 整32,
    y: 整32,
}

実装 点 {
    公開 関数 原点() -> 点 {
        点 { x: 0, y: 0 }
    }
}

関数 主() {
    束縛 p = 点::原点();
    もし p.x == 0 {
        表示!("原点です");
    }
}
```

### Romaji spelling

The same illustrative example using ASCII spellings. Long vowels are written by doubling the vowel, as in `kansuu` and `koukai`.

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

| Rust-inspired concept | Japanese | Romaji | Meaning |
|---|---|---|---|
| `fn` | 関数 | `kansuu` | Function |
| `let` | 束縛 | `sokubaku` | Binding |
| `mut` | 可変 | `kahen` | Mutable |
| `if` / `else` | もし / 他 | `moshi` / `hoka` | Conditional |
| `for … in` | 各 x 中 | `kaku x naka` | Iteration |
| `match` | 照合 | `shougou` | Pattern matching |
| `struct` | 構造体 | `kouzoutai` | Structure |
| `impl` | 実装 | `jissou` | Implementation |
| `return` | 返却 | `henkyaku` | Return |
| `println!` | 表示! | `hyouji!` | Display output |

## Getting started

### Requirements

- [Rust and Cargo](https://www.rust-lang.org/tools/install)

### Build and test

From the repository root, build and run the test suite:

```bash
cargo build --release
cargo test
```

The stage-zero compiler currently provides lexer-oriented commands:

```bash
# Tokenize a source file
 target/release/nihonc --tokens path/to/file.nihon

# Read romaji keywords without warnings
 target/release/nihonc --romaji path/to/file.nihon

# Treat warnings as errors
 target/release/nihonc --strict path/to/file.nihon
```

To run the compiler on a file with its default options:

```bash
target/release/nihonc path/to/file.nihon
```

For compiler-specific details and test instructions, see [`nihonc/README.md`](nihonc/README.md).

## Roadmap

- [x] Initial keyword table
- [x] Lexer
- [ ] Initial grammar specification
- [ ] Parser
- [ ] Type checking
- [ ] Decide on a backend
- [ ] Minimal standard library
- [ ] Rewrite the compiler in Nihon (stage 1)
- [ ] Bootstrap a compiler that can compile itself

## Contributing

Nihon is an experimental project, and feedback is welcome. Open an issue to discuss bugs, ideas, or Japanese and romaji keyword choices. Please keep in mind that language syntax and project plans may change as development progresses.

## License

Nihon is dual-licensed under the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
