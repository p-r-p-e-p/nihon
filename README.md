# Nihon for VS Code

Language support for **Nihon**, a Rust-style programming language written in kanji, hiragana and katakana (`.nihon` files).

```
関数 階乗(n: 整32) -> 整32 {
    もし n <= 1 { 返却 1; }
    n * 階乗(n - 1)
}

関数 主() {
    表示!("こんにちは世界");
}
```

## Features

- **Syntax highlighting** for kanji, kana and romaji keywords, macros (`表示!`), numbers, strings, characters and nested block comments.
- **Full-width character detection:** zenkaku punctuation, digits and letters (`；`, `１`, `（`…) are underlined in red, mirroring compiler error E0002. Half-width katakana is flagged like warning W0005.
- **Romaji completion:** type `kansuu` and pick `関数` from the list. Macros work the same way (`hyouji` → `表示!`).
- **Hover** shows the romaji spelling and English meaning of every keyword and macro.
- **Snippets** with Japanese and romaji prefixes (`main`, `kansuu`, `moshi`, `kaku`, `shougou`…).
- **Compiler diagnostics:** errors and warnings from `nihonc` appear in the editor when a file is opened or saved, with error codes, notes and help text.
- **Show tokens:** the *Nihon: Show tokens* command runs `nihonc --tokens` and opens the output beside the editor.

## Commands

| Command | Description |
| --- | --- |
| `Nihon: Check file` | Runs `nihonc` on the current file and updates the diagnostics. |
| `Nihon: Show tokens` | Saves the file and shows the lexer's token stream. |

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `nihon.compilerPath` | `nihonc` | Path to the compiler executable. |
| `nihon.diagnostics.enabled` | `true` | Run `nihonc` on open and save. |
| `nihon.romaji` | `false` | Pass `--romaji` (no warning for latin-letter keywords). |
| `nihon.strict` | `false` | Pass `--strict` (warnings become errors). |

## Requirements

Diagnostics and *Show tokens* need the `nihonc` compiler. Build it from the Nihon repository and either put it on your `PATH` or set `nihon.compilerPath`. Everything else works without it.
