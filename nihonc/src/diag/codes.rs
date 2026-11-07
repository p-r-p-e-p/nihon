//! Catalogo de codigos de erro e aviso (stage 0, parte lexica).
//! Os codigos nunca mudam de significado depois de publicados.

// ---- Lexico (E00xx) ----
/// Texto entre aspas sem fechar.
pub const E0001: &str = "E0001";
/// Pontuacao, digito ou espaco em largura total (zenkaku).
pub const E0002: &str = "E0002";
/// Palavra-chave colada num nome (falta um espaco).
pub const E0003: &str = "E0003";
// E0004 (palavra reservada usada como nome) nasce no parser.
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

// ---- Avisos ----
/// Palavra-chave escrita em letras latinas (romaji).
pub const W0001: &str = "W0001";
/// Katakana de meia largura em nome.
pub const W0005: &str = "W0005";
