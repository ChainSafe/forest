/// ExitCode defines the exit code from the VM execution
#[derive(PartialEq, Eq)]
pub struct ExitCode {
    pub(crate) success: bool,
    pub(crate) system_error: i32,
    pub(crate) user_defined_error: (),
}

impl ExitCode {
    /// returns true if the exit code was a success
    pub fn is_success(&self) -> bool {
        self.success
    }
    /// returns true if exited with an error code
    pub fn is_error(&self) -> bool {
        !self.success
    }
    /// returns true if the execution was successful
    pub fn allows_state_update(&self) -> bool {
        self.success
    }
}
