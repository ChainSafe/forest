use crate::ExitCode;

/// Output variables for actor method invocation.
pub struct InvocOutput {
    pub exit_code: ExitCode,
    pub return_value: Vec<u8>,
}

impl InvocOutput {
    pub fn create_error(code: ExitCode) -> Self {
        Self {
            exit_code: code,
            return_value: vec![],
        }
    }
}
