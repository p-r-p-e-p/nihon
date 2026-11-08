//! Catalogo de codigos de erro e aviso (stage 0, parte lexica).
//! Os codigos nunca mudam de significado depois de publicados.

// ---- Lexico (E00xx) ----
/// Texto entre aspas sem fechar.
pub const E0001: &str = "E0001";
/// Pontuacao, digito ou espaco em largura total (zenkaku).
pub const E0002: &str = "E0002";
/// Palavra-chave colada num nome (falta um espaco).
pub const E0003: &str = "E0003";
/// Palavra reservada usada como nome.
pub const E0004: &str = "E0004";
/// Literal numerico invalido.
pub const E0005: &str = "E0005";
/// Comentario de bloco sem fechar.
pub const E0006: &str = "E0006";
/// Caractere desconhecido.
pub const E0007: &str = "E0007";
/// Sequencia de escape invalida em texto ou caractere.
pub const E0008: &str = "E0008";
/// Literal de caractere invalido.
pub const E0009: &str = "E0009";

// ---- Sintaxe (E01xx) ----
/// Faltou `{`.
pub const E0101: &str = "E0101";
/// Faltou `;`.
pub const E0102: &str = "E0102";
/// Delimitador sem fechar, a mais ou trocado.
pub const E0103: &str = "E0103";
/// Faltou o tipo.
pub const E0104: &str = "E0104";
/// Faltou uma expressao.
pub const E0105: &str = "E0105";
/// Faltou o `中` no laco `各`.
pub const E0106: &str = "E0106";
/// `他` ou `他もし` sem um `もし` antes.
pub const E0107: &str = "E0107";
/// Esperava `関数` ou `定数` no nivel de cima.
pub const E0108: &str = "E0108";
/// Esperava um nome.
pub const E0109: &str = "E0109";
/// Outro token inesperado.
pub const E0110: &str = "E0110";

// ---- Nomes e verificacoes minimas (E02xx, E05xx) ----
/// Valor (variavel ou constante) nao encontrado.
pub const E0201: &str = "E0201";
/// Funcao nao encontrada.
pub const E0202: &str = "E0202";
/// Nome definido mais de uma vez.
pub const E0203: &str = "E0203";
/// Falta a funcao `主`.
pub const E0204: &str = "E0204";
/// `主` com assinatura errada.
pub const E0205: &str = "E0205";
/// Constante que depende de si mesma.
pub const E0206: &str = "E0206";
/// Chamada de funcao ou macro dentro de constante.
pub const E0207: &str = "E0207";
/// A conta de uma constante estoura ou divide por zero na compilacao.
pub const E0208: &str = "E0208";
/// Tipos diferentes.
pub const E0301: &str = "E0301";
/// Quantidade errada de argumentos.
pub const E0302: &str = "E0302";
/// Condicao que nao e `真偽`.
pub const E0303: &str = "E0303";
/// Operador nao se aplica aos tipos.
pub const E0304: &str = "E0304";
/// Conversao `として` invalida.
pub const E0305: &str = "E0305";
/// Literal inteiro fora do limite.
pub const E0306: &str = "E0306";
/// `;` a mais no fim da funcao.
pub const E0307: &str = "E0307";
/// Atribuicao a variavel imutavel.
pub const E0401: &str = "E0401";
/// `中断` ou `続行` fora de um laco.
pub const E0402: &str = "E0402";
/// Quantidade de `{}` diferente da de argumentos.
pub const E0501: &str = "E0501";
/// Macro desconhecida.
pub const E0502: &str = "E0502";
/// O texto de formato precisa ser um literal.
pub const E0503: &str = "E0503";
/// Texto de formato invalido.
pub const E0504: &str = "E0504";
/// Temporario: o gerador de C ainda nao suporta esta construcao.
/// Some conforme as proximas fatias entram.
pub const E0999: &str = "E0999";

// ---- Avisos ----
/// Palavra-chave escrita em letras latinas (romaji).
pub const W0001: &str = "W0001";
/// Variavel nunca usada.
pub const W0002: &str = "W0002";
/// Codigo depois de algo que nunca termina.
pub const W0003: &str = "W0003";
/// `可変` desnecessario.
pub const W0004: &str = "W0004";
/// Katakana de meia largura em nome.
pub const W0005: &str = "W0005";
