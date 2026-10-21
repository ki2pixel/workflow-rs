use std::ffi::NulError;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum TpuError {
    Io(std::io::Error),
    Nul(NulError),
    DlOpen {
        path: PathBuf,
        message: String,
    },
    Symbol {
        name: String,
        message: String,
    },
    ModelNotCompiled {
        path: PathBuf,
    },
    DelegateUnavailable,
    TfLite {
        context: &'static str,
        status: i32,
    },
    NullTensor {
        context: &'static str,
        index: i32,
    },
    WrongInputCount {
        expected: usize,
        got: usize,
    },
    WrongInputSize {
        index: usize,
        expected: usize,
        got: usize,
    },
}

impl fmt::Display for TpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Nul(e) => write!(f, "chaîne C invalide: {e}"),
            Self::DlOpen { path, message } => {
                write!(f, "dlopen({}) a échoué: {message}", path.display())
            }
            Self::Symbol { name, message } => write!(f, "symbole {name} introuvable: {message}"),
            Self::ModelNotCompiled { path } => write!(
                f,
                "{} ne contient pas la signature edgetpu-custom-op",
                path.display()
            ),
            Self::DelegateUnavailable => {
                write!(f, "edgetpu_create_delegate a retourné un pointeur nul")
            }
            Self::TfLite { context, status } => {
                write!(f, "{context} a échoué (TfLiteStatus {status})")
            }
            Self::NullTensor { context, index } => {
                write!(f, "{context} a retourné un tenseur nul à l'index {index}")
            }
            Self::WrongInputCount { expected, got } => {
                write!(
                    f,
                    "nombre d'entrées incorrect: attendu {expected}, reçu {got}"
                )
            }
            Self::WrongInputSize {
                index,
                expected,
                got,
            } => write!(
                f,
                "taille d'entrée incorrecte pour l'index {index}: attendu {expected}, reçu {got}"
            ),
        }
    }
}

impl std::error::Error for TpuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Nul(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TpuError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<NulError> for TpuError {
    fn from(value: NulError) -> Self {
        Self::Nul(value)
    }
}
