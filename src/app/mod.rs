use crate::commands::AppPhase;
use crate::config::AppConfig;

pub mod update;
pub mod view;

pub struct App {
    pub config: AppConfig,
    pub phase: AppPhase,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            config,
            phase: AppPhase::Initializing,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.phase = AppPhase::Running;
        update::run(self)?;
        Ok(())
    }
}
