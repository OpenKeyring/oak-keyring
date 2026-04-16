use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HealthCheckFrequency {
    #[default]
    OnStartup,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub health_check_enabled: bool,
    #[serde(default)]
    pub health_check_frequency: HealthCheckFrequency,
    #[serde(default = "default_true")]
    pub audit_enabled: bool,
    #[serde(default = "default_audit_days")]
    pub audit_retention_days: u32,
    #[serde(default)]
    pub rotation: crate::types::rotation::RotationConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            health_check_enabled: true,
            health_check_frequency: HealthCheckFrequency::OnStartup,
            audit_enabled: true,
            audit_retention_days: 365,
            rotation: crate::types::rotation::RotationConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_audit_days() -> u32 {
    365
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_config_default_includes_rotation_config() {
        let config = SecurityConfig::default();
        assert!(config.rotation.auto_rotate);
        assert_eq!(config.rotation.rotate_after_days, Some(90));
    }
}
