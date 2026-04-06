pub mod clipboard;
pub mod config;
pub mod execute;
pub mod health;
pub mod import_export;
pub mod record;
pub mod sync;
pub mod timer;
pub mod vault;

use crate::commands::{Command, Message};
use tokio::sync::mpsc;

pub struct CommandExecutor {
    #[allow(dead_code)]
    result_tx: mpsc::Sender<Message>,
}

impl CommandExecutor {
    pub fn new(result_tx: mpsc::Sender<Message>) -> Self {
        Self { result_tx }
    }

    pub async fn run(self, mut _command_rx: mpsc::Receiver<Command>) {
        // TODO: Implement executor run loop
    }

    pub async fn execute(&mut self, _command: Command) {
        // TODO: Implement command dispatch
    }
}
