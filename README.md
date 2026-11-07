<p align="center">
  <img src="assets/SubBanner.png" alt="Nihon — a Japanese-script systems programming language" width="720">
</p>

<h1 align="center">Nihon for VS Code</h1>

<p align="center">
  Language support for <strong>Nihon</strong>, a Rust-inspired systems programming language written with Japanese scripts.
</p>

<p align="center">
  <img src="assets/Hon.png" alt="Hon, the Nihon language mascot" width="150">
  <br>
  <em>Write Nihon in your editor, with a little help from Hon.</em>
</p>

<p align="center">
  <a href="#features"><img src="https://img.shields.io/badge/editor-VS%20Code-007ACC?logo=visualstudiocode" alt="VS Code extension"></a>
  <a href="#requirements"><img src="https://img.shields.io/badge/compiler-nihonc-orange" alt="Requires nihonc for compiler features"></a>
  <a href="../LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="MIT or Apache-2.0 license"></a>
</p>

## Overview

Nihon for VS Code brings editor support to `.nihon` files, combining Japanese-script and romaji language features with diagnostics from the Nihon compiler. The extension supports syntax highlighting, completion, hover information, snippets, and compiler commands.

The language and compiler are in early development, so syntax and editor features may evolve. For more about the language, compiler status, and project roadmap, see the [Nihon project README](../README.md).

## Example

```nihon
関数 階乗(n: 整32) -> 整32 {
    もし n <= 1 { 返却 1; }
    n * 階乗(n - 1)
}

関数 主() {
    表示!("こんにちは世界");
}
```

## Features

- **Syntax highlighting** for kanji, kana, and romaji keywords, macros (`表示!`), numbers, strings, characters, and nested block comments.
- **Full-width character detection:** flags zenkaku punctuation, digits, and letters (such as `；`, `１`, and `（`) to mirror compiler error E0002. Half-width katakana is also flagged.
- **Romaji completion:** type `kansuu` and choose `関数`; macros work the same way (`hyouji` → `表示!`).
- **Hover information** with the romaji spelling and English meaning of each keyword and macro.
- **Snippets** with Japanese and romaji prefixes, including `main`, `kansuu`, `moshi`, `kaku`, and `shougou`.
- **Compiler diagnostics:** errors and warnings from `nihonc` appear when a file is opened or saved, including error codes, notes, and help text.
- **Token inspection:** the *Nihon: Show tokens* command runs `nihonc --tokens` and opens the output beside the editor.

## Commands

| Command | Description |
| --- | --- |
| `Nihon: Check file` | Runs `nihonc` on the current file and updates the diagnostics. |
| `Nihon: Show tokens` | Saves the file and displays the lexer's token stream. |

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `nihon.compilerPath` | `nihonc` | Path to the compiler executable. |
| `nihon.diagnostics.enabled` | `true` | Run `nihonc` when a file is opened or saved. |
| `nihon.romaji` | `false` | Pass `--romaji` (do not warn about Latin-letter keywords). |
| `nihon.strict` | `false` | Pass `--strict` (warnings become errors). |

## Requirements

Diagnostics and *Show tokens* require the `nihonc` compiler. Build it from the Nihon repository and either add it to your `PATH` or set `nihon.compilerPath`. Syntax highlighting and the other editor features work without the compiler.

## License

Nihon is dual-licensed under the [MIT License](../LICENSE-MIT) or the [Apache License, Version 2.0](../LICENSE-APACHE), at your option.
