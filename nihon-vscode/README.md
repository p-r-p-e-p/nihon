# Nihon para VS Code

- Realce de sintaxe para `.nihon` (kanji, kana e romaji)
- Zenkaku (`；`, `（`, `Ａ`…) sublinhado em vermelho, como o erro E0002 do lexer; katakana de meia largura sublinhado como aviso (W0005)
- Snippets (prefixos em japonês e em romaji: `kansuu`, `moshi`, `kaku`, `main`…)
- Autocompletar por romaji: digite `kansuu` e escolha `関数`
- Hover com romaji e significado de cada palavra-chave
- Erros e avisos do `nihonc` ao abrir/salvar
- Comando "Nihon: Mostrar tokens" (roda `nihonc --tokens`)

Configuração: `nihon.compilerPath` (padrão `nihonc`), `nihon.romaji`, `nihon.strict`, `nihon.diagnostics.enabled`.
