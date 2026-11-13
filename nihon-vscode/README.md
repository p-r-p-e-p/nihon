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

- **Syntax highlighting** for kanji, kana and romaji keywords, types (`整8`…`整64`, `非負8`…`非負64`, `大きさ`, `実32`/`実64`, `真偽`, `文字`, `文字列` and their romaji forms), methods (`texto.長さ()`), enums and `照合` patterns (`色::赤`, `_`), macros (`表示!`), numbers, strings, characters and nested block comments.
- **Full-width character detection:** zenkaku punctuation, digits and letters (`；`, `１`, `（`…) are underlined in red, mirroring compiler error E0002. Half-width katakana is flagged like warning W0005.
- **Live diagnostics:** errors and warnings from `nihonc` appear as you type (after a short pause), with error codes, notes and help text.
- **Quick fixes:** press `Ctrl+.` on a diagnostic to apply the compiler's own suggestion, for example:
  - `１；` → `1;` (E0002), `もしx` → `もし x` (E0003)
  - `kansuu` → `関数` (W0001), `ﾊﾝｶｸ` → `ハンカク` (W0005)
  - missing `;` (E0102) or `中` in a `各` loop (E0106)
  - `mojiretsu` → `文字列`, `.nagasa()` → `.長さ()` (W0001 on types and methods)
  - remove an unneeded `可変` (W0004)
  - "did you mean" suggestions for misspelled names, types and macros (offered, never applied automatically)
- **Fix all safe issues:** applies every automatic fix in the file at once.
- **Romaji completion:** type `kansuu` and pick `関数` from the list. Types, macros and methods work the same way (`hyouji` → `表示!`, and after a dot `nagasa` → `長さ()`).
- **Hover** shows the romaji spelling and English meaning of every keyword, type, macro and method.
- **Snippets** with Japanese and romaji prefixes (`main`, `kansuu`, `moshi`, `kaku`, `teisuu`, `rekkyogata`, `shougou`…).
- **Show tokens:** runs `nihonc --tokens` and opens the output beside the editor.

## Commands

| Command | Description |
| --- | --- |
| `Nihon: Check file` | Runs `nihonc` on the current file and refreshes the diagnostics. |
| `Nihon: Fix all safe issues` | Applies every safe automatic fix in the current file. |
| `Nihon: Show tokens` | Saves the file and shows the lexer's token stream. |

To fix everything safe on save, add this to your `settings.json`:

```json
"[nihon]": {
  "editor.codeActionsOnSave": { "source.fixAll.nihon": "explicit" }
}
```

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `nihon.compilerPath` | `nihonc` | Path to the compiler executable. |
| `nihon.diagnostics.enabled` | `true` | Run `nihonc` and show its errors and warnings. |
| `nihon.diagnostics.run` | `onType` | `onType` checks while you type; `onSave` only when a file is opened or saved. |
| `nihon.romaji` | `false` | Pass `--romaji` (no warning for latin-letter keywords). |
| `nihon.strict` | `false` | Pass `--strict` (warnings become errors). |

## Requirements

Diagnostics, quick fixes and *Show tokens* need the `nihonc` compiler. Build it from the Nihon repository and either put it on your `PATH` or set `nihon.compilerPath`.

- Quick fixes need a `nihonc` that supports `--message-format=json`.
- With an older `nihonc`, diagnostics still work (read from the normal output) but without quick fixes.
- The extension runs the compiler with `--check`, so it never builds an executable.

Syntax highlighting, snippets, hover and completion work without the compiler.
