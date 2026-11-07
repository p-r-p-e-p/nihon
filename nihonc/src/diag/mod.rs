//! Diagnosticos: o centro do compilador. Todas as etapas mandam seus erros
//! e avisos para ca, e o `render` desenha tudo no estilo do rustc.

pub mod codes;
pub mod render;

use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub primary: bool,
}

/// Troca de texto sugerida. `safe` diz se um futuro `nihonc fix` pode
/// aplicar sozinho (true) ou se e so uma dica (false).
#[derive(Debug, Clone)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
    /// Ainda sem leitor: o `nihonc fix` (fatia futura) vai usar.
    #[allow(dead_code)]
    pub safe: bool,
}

#[derive(Debug, Clone)]
pub struct Help {
    pub message: String,
    pub edit: Option<Edit>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub code: &'static str,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub helps: Vec<Help>,
}

impl Diagnostic {
    fn new(level: Level, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            level,
            code,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }

    pub fn error(code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Level::Error, code, message)
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Level::Warning, code, message)
    }

    pub fn primary(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
            primary: true,
        });
        self
    }

    /// Rotulo secundario (`----`). Usado a partir da fase de nomes e tipos.
    #[allow(dead_code)]
    pub fn secondary(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
            primary: false,
        });
        self
    }

    pub fn note(mut self, message: impl Into<String>) -> Diagnostic {
        self.notes.push(message.into());
        self
    }

    /// Dica so em texto.
    pub fn help(mut self, message: impl Into<String>) -> Diagnostic {
        self.helps.push(Help {
            message: message.into(),
            edit: None,
        });
        self
    }

    /// Dica com uma troca de texto que sera mostrada no codigo.
    pub fn suggest(
        mut self,
        message: impl Into<String>,
        span: Span,
        replacement: impl Into<String>,
        safe: bool,
    ) -> Diagnostic {
        self.helps.push(Help {
            message: message.into(),
            edit: Some(Edit {
                span,
                replacement: replacement.into(),
                safe,
            }),
        });
        self
    }
}

/// Opcoes de severidade vindas da linha de comando.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    /// Todo aviso vira erro.
    pub strict: bool,
    /// Palavras-chave em romaji sao aceitas sem aviso.
    pub romaji: bool,
}

/// Recebe os diagnosticos das etapas e aplica as opcoes (`--strict`).
pub struct Handler {
    pub config: Config,
    pub diags: Vec<Diagnostic>,
}

impl Handler {
    pub fn new(config: Config) -> Handler {
        Handler {
            config,
            diags: Vec::new(),
        }
    }

    pub fn emit(&mut self, mut d: Diagnostic) {
        if d.level == Level::Warning && self.config.strict {
            d.level = Level::Error;
            d.message.push_str(" (denied by `--strict`)");
        }
        self.diags.push(d);
    }

    pub fn error_count(&self) -> usize {
        self.diags.iter().filter(|d| d.level == Level::Error).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diags.iter().filter(|d| d.level == Level::Warning).count()
    }
}
