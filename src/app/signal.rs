//! Unix signal handler for graceful/forced shutdown.
//!
//! Spawns a tokio task that listens for SIGINT/SIGTERM:
//! - First SIGINT  -> graceful shutdown (Message::ShutdownRequested { force: false })
//! - Second SIGINT -> forced quit (exit code 1)
//! - SIGTERM       -> forced shutdown (Message::ShutdownRequested { force: true })

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::Message;

/// Handle to the signal-handler tokio task. Dropping cancels the task.
pub struct SignalHandler {
    cancel: CancellationToken,
}

impl SignalHandler {
    /// Spawn the signal listener. Returns a handle that will keep the task alive.
    pub fn spawn(result_tx: mpsc::Sender<Message>) -> Self {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let tx_sigint = result_tx.clone();

        tokio::spawn(async move {
            let mut sigint_count: u8 = 0;

            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => {
                        tracing::debug!("signal handler cancelled, exiting");
                        break;
                    }
                    result = tokio::signal::ctrl_c() => {
                        if let Err(e) = result {
                            tracing::error!("signal error: {}", e);
                            break;
                        }
                        sigint_count += 1;

                        if sigint_count >= 2 {
                            tracing::warn!("second SIGINT received, forcing quit");
                            std::process::exit(1);
                        }

                        tracing::info!("SIGINT received, requesting graceful shutdown");
                        if tx_sigint.send(Message::ShutdownRequested { force: false }).await.is_err() {
                            // Receiver dropped — app is already shutting down.
                            break;
                        }
                    }
                }
            }
        });

        // Also spawn SIGTERM handler on unix platforms.
        #[cfg(unix)]
        {
            let cancel_term = cancel.clone();
            let tx_term = result_tx.clone();
            tokio::spawn(async move {
                let mut stream = match tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::terminate(),
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("failed to install SIGTERM handler: {}", e);
                        return;
                    }
                };

                tokio::select! {
                    _ = cancel_term.cancelled() => {}
                    _ = stream.recv() => {
                        tracing::warn!("SIGTERM received, requesting forced shutdown");
                        let _ = tx_term.send(Message::ShutdownRequested { force: true }).await;
                    }
                }
            });
        }

        Self { cancel }
    }
}

impl Drop for SignalHandler {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
