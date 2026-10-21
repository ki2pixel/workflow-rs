#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod ffi;
pub mod session;

pub use error::TpuError;
pub use session::{verify_edgetpu_model, Session, TensorSpec};

/// Marqueur présent dans la table `OperatorCodes` d'un modèle compilé pour l'Edge TPU.
pub const EDGETPU_SIGNATURE: &[u8] = b"edgetpu-custom-op";
