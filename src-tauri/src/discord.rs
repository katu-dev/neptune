use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use tauri::{AppHandle, Manager};

// Discord application client ID for Neptune.
// Using a placeholder — users can replace with their own app ID.
const DISCORD_CLIENT_ID: &str = "1234567890";

/// Tracks the last known activity so it can be re-applied after reconnect.
#[derive(Clone)]
enum LastActivity {
    None,
    Playing {
        title: String,
        artist: String,
        start_timestamp: i64,
    },
    Paused,
}

pub struct DiscordPresence {
    client: Option<DiscordIpcClient>,
    pub enabled: bool,
    last_activity: LastActivity,
    /// Shared cancel flag — set to `true` to stop the reconnect thread.
    cancel_reconnect: Arc<AtomicBool>,
}

impl DiscordPresence {
    pub fn new() -> Self {
        Self {
            client: None,
            enabled: true,
            last_activity: LastActivity::None,
            cancel_reconnect: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Enable or disable Discord Rich Presence.
    /// When disabled, clears any active activity and stops reconnect attempts.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // Signal the reconnect thread to stop.
            self.cancel_reconnect.store(true, Ordering::Relaxed);
            // Clear activity and disconnect.
            if let Some(ref mut c) = self.client {
                let _ = c.clear_activity();
                let _ = c.close();
            }
            self.client = None;
            self.last_activity = LastActivity::None;
        } else {
            // Re-enable: reset cancel flag so a new reconnect thread can run.
            self.cancel_reconnect.store(false, Ordering::Relaxed);
        }
    }

    /// Update Rich Presence to show a playing track.
    pub fn update_playing(&mut self, title: &str, artist: &str, start_timestamp: i64) {
        if !self.enabled {
            return;
        }
        self.last_activity = LastActivity::Playing {
            title: title.to_string(),
            artist: artist.to_string(),
            start_timestamp,
        };
        if let Some(ref mut client) = self.client {
            let act = activity::Activity::new()
                .details(title)
                .state(artist)
                .timestamps(activity::Timestamps::new().start(start_timestamp));
            if let Err(e) = client.set_activity(act) {
                eprintln!("[discord] set_activity failed: {e}");
                self.client = None;
            }
        }
    }

    /// Update Rich Presence to show paused state.
    pub fn update_paused(&mut self) {
        if !self.enabled {
            return;
        }
        self.last_activity = LastActivity::Paused;
        if let Some(ref mut client) = self.client {
            let act = activity::Activity::new().details("Paused");
            if let Err(e) = client.set_activity(act) {
                eprintln!("[discord] set_activity (paused) failed: {e}");
                self.client = None;
            }
        }
    }

    /// Clear the Rich Presence activity entirely.
    pub fn clear(&mut self) {
        if !self.enabled {
            return;
        }
        self.last_activity = LastActivity::None;
        if let Some(ref mut client) = self.client {
            if let Err(e) = client.clear_activity() {
                eprintln!("[discord] clear_activity failed: {e}");
                self.client = None;
            }
        }
    }

    /// Attempt to connect to Discord IPC. Returns `false` silently if Discord
    /// is not running.
    pub fn try_connect(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        let mut c = DiscordIpcClient::new(DISCORD_CLIENT_ID);
        if c.connect().is_ok() {
            self.client = Some(c);
            true
        } else {
            false
        }
    }

    /// Spawn a background thread that retries connecting every 30 seconds.
    /// The thread stops when `cancel_reconnect` is set (via `set_enabled(false)`).
    /// After a successful reconnect, re-applies the last known activity.
    pub fn schedule_reconnect(&mut self, app_handle: AppHandle) {
        if !self.enabled {
            return;
        }
        // Reset cancel flag for this new reconnect cycle.
        self.cancel_reconnect.store(false, Ordering::Relaxed);
        let cancel = Arc::clone(&self.cancel_reconnect);
        let last_activity = self.last_activity.clone();

        std::thread::spawn(move || {
            loop {
                // Sleep 30 seconds, checking cancel flag every 500ms.
                let mut slept = 0u64;
                while slept < 30_000 {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(500));
                    slept += 500;
                }

                if cancel.load(Ordering::Relaxed) {
                    return;
                }

                // Try to connect.
                let mut c = DiscordIpcClient::new(DISCORD_CLIENT_ID);
                if c.connect().is_ok() {
                    // Re-apply last known activity.
                    match &last_activity {
                        LastActivity::Playing { title, artist, start_timestamp } => {
                            let act = activity::Activity::new()
                                .details(title.as_str())
                                .state(artist.as_str())
                                .timestamps(activity::Timestamps::new().start(*start_timestamp));
                            let _ = c.set_activity(act);
                        }
                        LastActivity::Paused => {
                            let act = activity::Activity::new().details("Paused");
                            let _ = c.set_activity(act);
                        }
                        LastActivity::None => {}
                    }

                    // Store the reconnected client via the app handle.
                    if let Some(discord_state) = app_handle.try_state::<std::sync::Mutex<DiscordPresence>>() {
                        let mut dp = discord_state.lock().unwrap();
                        if dp.enabled {
                            dp.client = Some(c);
                        }
                    }
                    return;
                }
                // Connection failed — loop and retry.
            }
        });
    }
}

/// Return the current Unix timestamp in seconds (for use as start_timestamp).
pub fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
