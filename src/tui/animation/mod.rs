pub mod effects;
pub mod transitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationLevel {
    Full,
    Reduced,
    None,
}

pub fn detect_animation_level() -> AnimationLevel {
    // TODO: Detect terminal capabilities
    AnimationLevel::Full
}
