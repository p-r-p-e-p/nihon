# nihonc

Compilador da linguagem **Nihon** (`.nihon`). Estado: stage 0, fatia 2 (lexer, parser, verificações iniciais e gerador de C). Sem dependências externas.

## Compilar e testar

Na raiz do repositório, execute:

```bash
cargo build --manifest-path nihonc/Cargo.toml --release
cargo test --manifest-path nihonc/Cargo.toml
```

Ou, dentro deste diretório (`nihonc/`):

```bash
cargo build --release
cargo test                      # testes unitários, de diagnóstico e de ponta a ponta
```

Para executar sem depender de um caminho fixo para o binário, use `cargo run` na raiz do repositório:

```bash
cargo run --manifest-path nihonc/Cargo.toml -- arquivo.nihon
cargo run --manifest-path nihonc/Cargo.toml -- --check arquivo.nihon    # só verifica; não gera executável
cargo run --manifest-path nihonc/Cargo.toml -- --emit-c arquivo.nihon   # mostra o C gerado
cargo run --manifest-path nihonc/Cargo.toml -- --ast arquivo.nihon      # mostra a árvore sintática
cargo run --manifest-path nihonc/Cargo.toml -- --tokens arquivo.nihon
cargo run --manifest-path nihonc/Cargo.toml -- --romaji arquivo.nihon   # aceita kansuu, moshi... sem aviso
cargo run --manifest-path nihonc/Cargo.toml -- --strict arquivo.nihon   # todo aviso vira erro
cargo run --manifest-path nihonc/Cargo.toml -- -o saida arquivo.nihon
```

Esses comandos também funcionam após o build. Se preferir executar o binário diretamente, estando na raiz do repositório use `nihonc/target/release/nihonc`; estando no diretório `nihonc/`, use `target/release/nihonc`. Esse arquivo só existe depois de compilar o projeto.

É necessário ter `cc`, `gcc` ou `clang` para compilar o executável gerado. `--check`, `--ast`, `--tokens` e `--emit-c` não precisam compilar um executável C.

## Exemplo

(`//` = leitura em romaji e significado)

```nihon
関数 主() {                      // kansuu shu() = função principal
    表示!("こんにちは世界");     // hyouji!("konnichiwa sekai") = exibe "olá, mundo"
}
```

## Estrutura

- `src/source.rs`: texto, spans em bytes e largura de colunas (kanji = 2)
- `src/width_table.rs`: gerado por `tools/gen_width.py` (não editar à mão)
- `src/diag/`: diagnósticos, catálogo de códigos e apresentação dos erros
- `src/lexer/`: lexer, classes de caracteres e tabela de palavras (japonês + romaji)
- `tests/ui/`: um `.nihon` e um `.stderr` esperado por caso (`BLESS=1 cargo test --test ui` regrava os arquivos esperados)
- `src/ast.rs`, `src/parser.rs`: árvore sintática e parser (descida recursiva + Pratt, com recuperação de erros)
- `src/codegen_c.rs`: verificações de nomes, tipos e mutabilidade, além da geração de C
- `tests/run.rs`: testes de ponta a ponta (compilam, executam e comparam a saída; precisam de um compilador C)

## Limites conhecidos do stage 0

- Só existem os tipos `整32`, `非負32`, `実64`, `真偽` e `文字`. Texto (`文字列`) só é aceito como primeiro argumento de `表示!`.
- Estouro de inteiro e divisão por zero durante a execução podem causar panic; o código de saída documentado é 101.
- Constantes são calculadas na compilação (números, `真偽`, `文字`, outras constantes, operadores, `として`, `もし`/`他`); estouro ou divisão por zero gera o erro E0208.
- `真偽` é impresso como `真 (shin)` / `偽 (gi)`.
