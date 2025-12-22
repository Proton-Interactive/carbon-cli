use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncCommand {
    Import,
    Export,
    Sourcemap,
}

#[derive(Clone)]
pub struct AppState {
    // We use a Mutex to allow safe concurrent access to the pending command.
    // This allows the CLI/Zed to set a command, and the Roblox plugin to poll and retrieve it.
    pub pending_command: Arc<Mutex<Option<SyncCommand>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pending_command: Arc::new(Mutex::new(None)),
        }
    }

    /// Sets the current pending command. Overwrites any existing command.
    pub fn set_command(&self, command: SyncCommand) {
        let mut lock = self.pending_command.lock().expect("Failed to lock mutex");
        *lock = Some(command);
    }

    /// Retrieves and clears the current pending command.
    pub fn pop_command(&self) -> Option<SyncCommand> {
        let mut lock = self.pending_command.lock().expect("Failed to lock mutex");
        lock.take()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
