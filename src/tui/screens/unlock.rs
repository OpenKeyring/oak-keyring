// TODO: Implement unlock screen per U1 spec

/// Unlock screen state: master password input with error display.
#[derive(Debug, Default)]
pub struct UnlockScreen {
    pub password_input: String,
    pub show_error: bool,
    pub error_message: String,
}
