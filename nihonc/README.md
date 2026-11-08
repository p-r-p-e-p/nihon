# nihonc

Compilador da linguagem **Nihon** (`.nihon`). Estado: stage 0, fatia 2 (lexer, parser, gerador de C).
Zero dependencias externas.

```
cargo build --release
cargo test                      # unitarios + testes de saida dos erros
target/release/nihonc arquivo.nihon            # gera o executavel `arquivo` (usa cc/gcc/clang)
target/release/nihonc -o saida arquivo.nihon
target/release/nihonc --check arquivo.nihon    # so confere, nao gera nada
target/release/nihonc --emit-c arquivo.nihon   # mostra o C gerado
target/release/nihonc --ast arquivo.nihon      # mostra a arvore sintatica
target/release/nihonc --tokens arquivo.nihon
target/release/nihonc --romaji arquivo.nihon   # aceita kansuu, moshi... sem aviso
target/release/nihonc --strict arquivo.nihon   # todo aviso vira erro
```

Exemplo (`//` = leitura em romaji e significado):

```
関数 主() {                      // kansuu shu() = funcao principal
    表示!("こんにちは世界");     // hyouji!("konnichiwa sekai") = exibe "ola mundo"
}
```

## Estrutura

- `src/source.rs` texto, spans em bytes, largura de colunas (kanji = 2)
- `src/width_table.rs` gerado por `tools/gen_width.py` (nao editar a mao)
- `src/diag/` diagnosticos, catalogo de codigos, desenho dos erros
- `src/lexer/` lexer, classes de caracteres, tabela de palavras (japones + romaji)
- `tests/ui/` um `.nihon` + `.stderr` esperado por caso (`BLESS=1 cargo test --test ui` regrava)
- `src/ast.rs`, `src/parser.rs` arvore e parser (descida recursiva + Pratt, com recuperacao de erros)
- `src/codegen_c.rs` verificacao de nomes/tipos/mutabilidade e geracao de C
- `tests/run.rs` testes de ponta a ponta (compila, roda, compara a saida; precisam de um compilador de C)

## Limites conhecidos do stage 0

- So existem `整32`, `非負32`, `実64`, `真偽`, `文字`; texto (`文字列`) so como primeiro argumento de `表示!`.
- Estouro de inteiro e divisao por zero dao panic (codigo de saida 101), como o Rust em debug.
- Constantes sao calculadas na compilacao (numeros, `真偽`, `文字`, outras constantes, operadores, `として`, `もし`/`他`); estouro ou divisao por zero e erro E0208.
- `真偽` e impresso como `真 (shin)` / `偽 (gi)`.
