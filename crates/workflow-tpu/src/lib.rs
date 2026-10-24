#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod ffi;
pub mod session;

pub use error::TpuError;
pub use session::{verify_edgetpu_model, Session, TensorSpec};

/// Marqueur présent dans la table `OperatorCodes` d'un modèle compilé pour l'Edge TPU.
pub const EDGETPU_SIGNATURE: &[u8] = b"edgetpu-custom-op";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn given_non_compiled_model_when_verified_then_named_error() {
        let error = verify_edgetpu_model(Path::new("/nonexistent/model.tflite")).unwrap_err();
        assert!(matches!(error, TpuError::Io(_)));
    }

    #[test]
    fn given_wrong_tensor_size_when_displayed_then_contract_fields_are_present() {
        let error = TpuError::WrongInputSize {
            index: 2,
            expected: 192,
            got: 128,
        };
        let message = error.to_string();
        assert!(message.contains("192") && message.contains("128"));
    }
}
