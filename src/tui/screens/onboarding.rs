// TODO: Implement onboarding wizard per U1 spec

/// Onboarding wizard state: step-by-step initial setup flow.
#[derive(Debug, Default)]
pub struct OnboardingScreen {
    pub current_step: u8,
    pub password_input: String,
    pub confirm_input: String,
}
