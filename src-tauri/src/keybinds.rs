use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::{db, types::AppError};

// ---------------------------------------------------------------------------
// ConflictError
// ---------------------------------------------------------------------------

/// Returned when `set_keybind` is called with a combo already bound to a
/// different action.
#[derive(Debug, Serialize)]
pub struct ConflictError {
    pub combo: String,
    pub existing_action: String,
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Key combo '{}' is already bound to '{}'",
            self.combo, self.existing_action
        )
    }
}

// ---------------------------------------------------------------------------
// KeybindMap
// ---------------------------------------------------------------------------

/// Maps action names to key combo strings (e.g. `"play_pause"` → `"Space"`).
#[derive(Clone, Serialize, Deserialize)]
pub struct KeybindMap(pub HashMap<String, String>);

impl KeybindMap {
    /// Return the default keybind mapping.
    pub fn defaults() -> Self {
        let mut map = HashMap::new();
        map.insert("play_pause".to_string(), "Space".to_string());
        map.insert("next_track".to_string(), "ArrowRight".to_string());
        map.insert("prev_track".to_string(), "ArrowLeft".to_string());
        map.insert("volume_up".to_string(), "ArrowUp".to_string());
        map.insert("volume_down".to_string(), "ArrowDown".to_string());
        map.insert("seek_forward".to_string(), "KeyF".to_string());
        map.insert("seek_backward".to_string(), "KeyB".to_string());
        map.insert("command_palette".to_string(), "Ctrl+KeyK".to_string());
        KeybindMap(map)
    }

    /// Look up which action is bound to the given key combo.
    pub fn get_action(&self, combo: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, v)| v.as_str() == combo)
            .map(|(k, _)| k.as_str())
    }

    /// Bind `action` to `combo`, overwriting any previous binding for that action.
    pub fn set(&mut self, action: &str, combo: &str) {
        self.0.insert(action.to_string(), combo.to_string());
    }

    /// Returns `true` if `combo` is already bound to a *different* action.
    pub fn has_conflict(&self, combo: &str, action: &str) -> bool {
        self.0
            .iter()
            .any(|(a, c)| c.as_str() == combo && a.as_str() != action)
    }
}

// ---------------------------------------------------------------------------
// KeybindRegistry
// ---------------------------------------------------------------------------

/// Tauri managed state that owns the live keybind map and handles persistence.
pub struct KeybindRegistry {
    map: Arc<Mutex<KeybindMap>>,
    app_handle: AppHandle,
}

impl KeybindRegistry {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            map: Arc::new(Mutex::new(KeybindMap::defaults())),
            app_handle,
        }
    }

    /// Load the persisted keybind map from the `app_state` table.
    /// Falls back to defaults if the key is absent or the JSON is invalid.
    pub fn load_from_db(&self) -> Result<(), AppError> {
        let conn = db::init_db(&self.app_handle)?;
        let result: rusqlite::Result<String> = conn.query_row(
            "SELECT value FROM app_state WHERE key = 'keybinds'",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(json) => {
                if let Ok(loaded) = serde_json::from_str::<KeybindMap>(&json) {
                    let mut map = self.map.lock().unwrap();
                    *map = loaded;
                }
                // If JSON is invalid, silently keep defaults.
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // No persisted keybinds — keep defaults.
            }
            Err(e) => return Err(AppError::Database(e.to_string())),
        }

        Ok(())
    }

    /// Persist the current keybind map to the `app_state` table.
    pub fn save_to_db(&self) -> Result<(), AppError> {
        let json = {
            let map = self.map.lock().unwrap();
            serde_json::to_string(&*map)
                .map_err(|e| AppError::Database(e.to_string()))?
        };

        let conn = db::init_db(&self.app_handle)?;
        conn.execute(
            "INSERT INTO app_state (key, value) VALUES ('keybinds', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![json],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Look up the action for `combo` and emit a `keybind_action` event.
    /// Returns `true` if an action was found and dispatched.
    pub fn dispatch(&self, combo: &str) -> bool {
        let action = {
            let map = self.map.lock().unwrap();
            map.get_action(combo).map(|s| s.to_string())
        };

        if let Some(action) = action {
            let _ = self.app_handle.emit("keybind_action", &action);
            true
        } else {
            false
        }
    }

    /// Reset the keybind map to defaults and persist.
    pub fn reset_to_defaults(&self) -> Result<(), AppError> {
        {
            let mut map = self.map.lock().unwrap();
            *map = KeybindMap::defaults();
        }
        self.save_to_db()
    }

    /// Return a clone of the current keybind map.
    pub fn get_map(&self) -> KeybindMap {
        self.map.lock().unwrap().clone()
    }

    /// Set a single keybind, checking for conflicts first.
    /// Returns `Err(ConflictError)` if the combo is already bound to a different action.
    pub fn set_keybind(&self, action: &str, combo: &str) -> Result<(), ConflictError> {
        let mut map = self.map.lock().unwrap();
        if map.has_conflict(combo, action) {
            let existing = map.get_action(combo).unwrap_or("").to_string();
            return Err(ConflictError {
                combo: combo.to_string(),
                existing_action: existing,
            });
        }
        map.set(action, combo);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Return the current keybind map.
#[tauri::command]
pub fn get_keybinds(
    registry: tauri::State<'_, KeybindRegistry>,
) -> KeybindMap {
    registry.get_map()
}

/// Assign a new key combo to an action.
/// Returns a `ConflictError` if the combo is already bound to a different action.
#[tauri::command]
pub fn set_keybind(
    action: String,
    combo: String,
    registry: tauri::State<'_, KeybindRegistry>,
) -> Result<(), ConflictError> {
    registry.set_keybind(&action, &combo)?;
    // Persist — ignore DB errors here so the in-memory change is still applied.
    let _ = registry.save_to_db();
    Ok(())
}

/// Reset all keybinds to their defaults and persist.
#[tauri::command]
pub fn reset_keybinds(
    registry: tauri::State<'_, KeybindRegistry>,
) -> Result<(), AppError> {
    registry.reset_to_defaults()
}

/// Dispatch a key combo: look up the bound action, emit `keybind_action`, and
/// return the action name (or `None` if no binding exists).
#[tauri::command]
pub fn dispatch_keybind(
    combo: String,
    registry: tauri::State<'_, KeybindRegistry>,
) -> Option<String> {
    let action = {
        let map = registry.map.lock().unwrap();
        map.get_action(&combo).map(|s| s.to_string())
    };

    if let Some(ref a) = action {
        let _ = registry.app_handle.emit("keybind_action", a);
    }

    action
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_map() -> KeybindMap {
        KeybindMap::defaults()
    }

    #[test]
    fn test_defaults_has_eight_bindings() {
        let map = default_map();
        assert_eq!(map.0.len(), 8);
    }

    #[test]
    fn test_defaults_play_pause_is_space() {
        let map = default_map();
        assert_eq!(map.0.get("play_pause").map(|s| s.as_str()), Some("Space"));
    }

    #[test]
    fn test_get_action_finds_combo() {
        let map = default_map();
        assert_eq!(map.get_action("Space"), Some("play_pause"));
        assert_eq!(map.get_action("ArrowRight"), Some("next_track"));
        assert_eq!(map.get_action("Ctrl+KeyK"), Some("command_palette"));
    }

    #[test]
    fn test_get_action_unknown_combo_returns_none() {
        let map = default_map();
        assert_eq!(map.get_action("KeyZ"), None);
    }

    #[test]
    fn test_set_overwrites_action() {
        let mut map = default_map();
        map.set("play_pause", "KeyP");
        assert_eq!(map.0.get("play_pause").map(|s| s.as_str()), Some("KeyP"));
        assert_eq!(map.get_action("KeyP"), Some("play_pause"));
    }

    #[test]
    fn test_has_conflict_detects_existing_combo() {
        let map = default_map();
        // "Space" is bound to "play_pause"; assigning it to "next_track" is a conflict.
        assert!(map.has_conflict("Space", "next_track"));
    }

    #[test]
    fn test_has_conflict_same_action_is_not_conflict() {
        let map = default_map();
        // Re-assigning "Space" to "play_pause" itself is not a conflict.
        assert!(!map.has_conflict("Space", "play_pause"));
    }

    #[test]
    fn test_has_conflict_unbound_combo_is_not_conflict() {
        let map = default_map();
        assert!(!map.has_conflict("KeyZ", "play_pause"));
    }

    #[test]
    fn test_all_default_combos_are_retrievable() {
        let map = default_map();
        let expected = [
            ("play_pause", "Space"),
            ("next_track", "ArrowRight"),
            ("prev_track", "ArrowLeft"),
            ("volume_up", "ArrowUp"),
            ("volume_down", "ArrowDown"),
            ("seek_forward", "KeyF"),
            ("seek_backward", "KeyB"),
            ("command_palette", "Ctrl+KeyK"),
        ];
        for (action, combo) in &expected {
            assert_eq!(
                map.get_action(combo),
                Some(*action),
                "combo '{}' should map to '{}'",
                combo,
                action
            );
        }
    }
}
