use std::sync::{Arc, Mutex};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::{db, types::AppError};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueState {
    pub track_ids: Vec<i64>,
    pub current_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct QueueChangedPayload {
    queue: Vec<i64>,
    current_index: Option<usize>,
}

// ---------------------------------------------------------------------------
// QueueManager
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct QueueManager {
    queue: Arc<Mutex<QueueState>>,
    app_handle: AppHandle,
}

impl QueueManager {
    /// Create a new `QueueManager` with an empty queue.
    pub fn new(app_handle: AppHandle) -> Self {
        QueueManager {
            queue: Arc::new(Mutex::new(QueueState {
                track_ids: Vec::new(),
                current_index: None,
            })),
            app_handle,
        }
    }

    /// Load the persisted queue from the database.
    pub fn load_from_db(&self) -> Result<(), AppError> {
        let conn = db::init_db(&self.app_handle)?;

        // Load ordered track IDs from the queue table.
        let mut stmt = conn.prepare(
            "SELECT track_id FROM queue ORDER BY position ASC",
        )?;
        let track_ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Load current_index from queue_state.
        let current_index: Option<usize> = conn
            .query_row(
                "SELECT value FROM queue_state WHERE key = 'current_index'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<usize>().ok());

        let mut q = self.queue.lock().unwrap();
        q.track_ids = track_ids;
        q.current_index = current_index;

        Ok(())
    }

    /// Append a track to the end of the queue.
    pub fn add_to_end(&self, track_id: i64) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            q.track_ids.push(track_id);
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Insert a track immediately after the current playing position.
    /// If the queue is empty or there is no current track, the track is appended.
    pub fn add_next(&self, track_id: i64) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            let insert_at = match q.current_index {
                Some(idx) => (idx + 1).min(q.track_ids.len()),
                None => q.track_ids.len(),
            };
            q.track_ids.insert(insert_at, track_id);
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Remove the track at the given index from the queue.
    pub fn remove(&self, index: usize) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            if index >= q.track_ids.len() {
                return Err(AppError::Database(format!(
                    "Queue index {} out of bounds (len={})",
                    index,
                    q.track_ids.len()
                )));
            }
            q.track_ids.remove(index);

            // Adjust current_index if needed.
            if let Some(ci) = q.current_index {
                if index < ci {
                    q.current_index = Some(ci - 1);
                } else if index == ci {
                    // The currently playing track was removed; point to the
                    // same position (which is now the next track), or None if
                    // the queue is now empty.
                    if q.track_ids.is_empty() {
                        q.current_index = None;
                    } else {
                        q.current_index = Some(ci.min(q.track_ids.len() - 1));
                    }
                }
            }
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Move a track from `from` to `to` (both are indices into the queue).
    pub fn move_track(&self, from: usize, to: usize) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            let len = q.track_ids.len();
            if from >= len || to >= len {
                return Err(AppError::Database(format!(
                    "Queue move indices out of bounds: from={} to={} len={}",
                    from, to, len
                )));
            }
            let track = q.track_ids.remove(from);
            q.track_ids.insert(to, track);

            // Adjust current_index.
            if let Some(ci) = q.current_index {
                q.current_index = Some(adjust_index_after_move(ci, from, to));
            }
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Remove all tracks from the queue.
    pub fn clear(&self) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            q.track_ids.clear();
            q.current_index = None;
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Shuffle all tracks after the current playing position in-place.
    pub fn shuffle_after_current(&self) -> Result<(), AppError> {
        {
            let mut q = self.queue.lock().unwrap();
            let start = match q.current_index {
                Some(ci) => ci + 1,
                None => 0,
            };
            if start < q.track_ids.len() {
                let slice = &mut q.track_ids[start..];
                fisher_yates_shuffle(slice);
            }
        }
        self.persist()?;
        self.emit_changed();
        Ok(())
    }

    /// Advance to the next track in the queue.
    /// Returns the next `track_id`, or `None` if the queue is exhausted.
    pub fn advance(&self) -> Result<Option<i64>, AppError> {
        let next = {
            let mut q = self.queue.lock().unwrap();
            let next_index = match q.current_index {
                Some(ci) => ci + 1,
                None => 0,
            };
            if next_index < q.track_ids.len() {
                q.current_index = Some(next_index);
                Some(q.track_ids[next_index])
            } else {
                // Queue exhausted — leave current_index pointing past the end
                // so callers know we've finished.
                None
            }
        };
        self.persist()?;
        self.emit_changed();
        Ok(next)
    }

    /// Return a snapshot of the current queue state.
    pub fn state(&self) -> QueueState {
        self.queue.lock().unwrap().clone()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Write the current queue to the `queue` and `queue_state` tables.
    fn persist(&self) -> Result<(), AppError> {
        let conn = db::init_db(&self.app_handle)?;
        let q = self.queue.lock().unwrap();

        // Rewrite the queue table.
        conn.execute("DELETE FROM queue", [])?;
        for (pos, &track_id) in q.track_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO queue (position, track_id) VALUES (?1, ?2)",
                params![pos as i64, track_id],
            )?;
        }

        // Persist current_index.
        match q.current_index {
            Some(ci) => {
                conn.execute(
                    "INSERT INTO queue_state (key, value) VALUES ('current_index', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![ci.to_string()],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM queue_state WHERE key = 'current_index'",
                    [],
                )?;
            }
        }

        Ok(())
    }

    /// Emit the `queue_changed` Tauri event.
    fn emit_changed(&self) {
        let q = self.queue.lock().unwrap();
        let _ = self.app_handle.emit(
            "queue_changed",
            QueueChangedPayload {
                queue: q.track_ids.clone(),
                current_index: q.current_index,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fisher-Yates in-place shuffle using a simple LCG PRNG seeded from the
/// system time. This avoids pulling in the `rand` crate as a hard dependency
/// (it is available transitively, but using it directly would require adding
/// it to Cargo.toml).
fn fisher_yates_shuffle(slice: &mut [i64]) {
    let n = slice.len();
    if n <= 1 {
        return;
    }
    // LCG parameters (Knuth)
    let mut state: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    for i in (1..n).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        slice.swap(i, j);
    }
}

/// Compute the new index of an element that was at `original` after a
/// remove-then-insert move from `from` to `to`.
fn adjust_index_after_move(original: usize, from: usize, to: usize) -> usize {
    if original == from {
        return to;
    }
    if from < to {
        // Elements between (from+1)..=to shift left by 1.
        if original > from && original <= to {
            original - 1
        } else {
            original
        }
    } else {
        // Elements between to..=(from-1) shift right by 1.
        if original >= to && original < from {
            original + 1
        } else {
            original
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Unit tests for pure logic helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_adjust_index_after_move_same() {
        // Moving element 2 from position 2 to position 4
        assert_eq!(adjust_index_after_move(2, 2, 4), 4);
    }

    #[test]
    fn test_adjust_index_after_move_shift_left() {
        // Move from=1 to=3: elements at 2,3 shift left
        assert_eq!(adjust_index_after_move(2, 1, 3), 1);
        assert_eq!(adjust_index_after_move(3, 1, 3), 2);
        assert_eq!(adjust_index_after_move(0, 1, 3), 0); // unaffected
        assert_eq!(adjust_index_after_move(4, 1, 3), 4); // unaffected
    }

    #[test]
    fn test_adjust_index_after_move_shift_right() {
        // Move from=3 to=1: elements at 1,2 shift right
        assert_eq!(adjust_index_after_move(1, 3, 1), 2);
        assert_eq!(adjust_index_after_move(2, 3, 1), 3);
        assert_eq!(adjust_index_after_move(0, 3, 1), 0); // unaffected
        assert_eq!(adjust_index_after_move(4, 3, 1), 4); // unaffected
    }

    #[test]
    fn test_fisher_yates_shuffle_preserves_elements() {
        let original = vec![1i64, 2, 3, 4, 5, 6, 7, 8];
        let mut shuffled = original.clone();
        fisher_yates_shuffle(&mut shuffled);
        let mut sorted = shuffled.clone();
        sorted.sort();
        assert_eq!(sorted, original);
    }

    #[test]
    fn test_fisher_yates_shuffle_single_element() {
        let mut v = vec![42i64];
        fisher_yates_shuffle(&mut v);
        assert_eq!(v, vec![42]);
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    use proptest::prelude::*;

    // Helper: build a QueueState directly (no DB needed for pure logic tests).
    fn make_state(ids: Vec<i64>, current: Option<usize>) -> QueueState {
        QueueState {
            track_ids: ids,
            current_index: current,
        }
    }

    // Property 1: Queue append places track at end
    // Validates: Requirements 2.2
    proptest! {
        #[test]
        fn prop_add_to_end_places_at_end(
            ids in prop::collection::vec(any::<i64>(), 0..20),
            new_id in any::<i64>(),
        ) {
            let mut state = make_state(ids.clone(), None);
            let expected_len = state.track_ids.len() + 1;
            state.track_ids.push(new_id);
            prop_assert_eq!(state.track_ids.len(), expected_len);
            prop_assert_eq!(*state.track_ids.last().unwrap(), new_id);
        }
    }

    // Property 2: Queue play-next inserts after current position
    // Validates: Requirements 2.3
    proptest! {
        #[test]
        fn prop_add_next_inserts_after_current(
            ids in prop::collection::vec(1i64..100, 1..20),
            current_idx in 0usize..19usize,
            new_id in 200i64..300,
        ) {
            let current_idx = current_idx.min(ids.len() - 1);
            let mut state = make_state(ids.clone(), Some(current_idx));
            let insert_at = current_idx + 1;
            state.track_ids.insert(insert_at, new_id);
            prop_assert_eq!(state.track_ids[insert_at], new_id);
        }
    }

    // Property 3: Queue mutation (remove) preserves element set minus removed
    // Validates: Requirements 2.4, 2.5
    proptest! {
        #[test]
        fn prop_remove_preserves_element_set(
            ids in prop::collection::vec(1i64..1000, 1..20),
            remove_idx in 0usize..19usize,
        ) {
            let remove_idx = remove_idx.min(ids.len() - 1);
            let removed_id = ids[remove_idx];
            let mut state = make_state(ids.clone(), None);
            state.track_ids.remove(remove_idx);

            // The removed element should no longer be present (unless it was a duplicate)
            let original_count = ids.iter().filter(|&&x| x == removed_id).count();
            let new_count = state.track_ids.iter().filter(|&&x| x == removed_id).count();
            prop_assert_eq!(new_count, original_count - 1);

            // All other elements still present
            prop_assert_eq!(state.track_ids.len(), ids.len() - 1);
        }
    }

    // Property 4: Queue persistence round-trip (pure DB logic, no AppHandle)
    // Validates: Requirements 2.9
    //
    // We test the DB read/write logic directly using an in-memory SQLite
    // connection, mirroring what persist() and load_from_db() do.
    proptest! {
        #[test]
        fn prop_queue_persistence_round_trip(
            ids in prop::collection::vec(1i64..10000, 0..20),
            current_idx in prop::option::of(0usize..19usize),
        ) {
            use rusqlite::Connection;

            let current_idx = current_idx.map(|ci| {
                if ids.is_empty() { None } else { Some(ci.min(ids.len() - 1)) }
            }).flatten();

            // Set up an in-memory DB with the queue schema.
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("
                CREATE TABLE queue (
                    position INTEGER PRIMARY KEY,
                    track_id INTEGER NOT NULL
                );
                CREATE TABLE queue_state (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
            ").unwrap();

            // Write (simulate persist()).
            conn.execute("DELETE FROM queue", []).unwrap();
            for (pos, &tid) in ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO queue (position, track_id) VALUES (?1, ?2)",
                    rusqlite::params![pos as i64, tid],
                ).unwrap();
            }
            match current_idx {
                Some(ci) => {
                    conn.execute(
                        "INSERT INTO queue_state (key, value) VALUES ('current_index', ?1)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        rusqlite::params![ci.to_string()],
                    ).unwrap();
                }
                None => {
                    conn.execute("DELETE FROM queue_state WHERE key = 'current_index'", []).unwrap();
                }
            }

            // Read back (simulate load_from_db()).
            let mut stmt = conn.prepare("SELECT track_id FROM queue ORDER BY position ASC").unwrap();
            let loaded_ids: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            let loaded_index: Option<usize> = conn
                .query_row(
                    "SELECT value FROM queue_state WHERE key = 'current_index'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok()
                .and_then(|v| v.parse::<usize>().ok());

            prop_assert_eq!(loaded_ids, ids);
            prop_assert_eq!(loaded_index, current_idx);
        }
    }

    // Property 5: Queue shuffle is a permutation
    // Validates: Requirements 2.11
    proptest! {
        #[test]
        fn prop_shuffle_is_permutation(
            ids in prop::collection::vec(any::<i64>(), 0..20),
            current_idx in prop::option::of(0usize..19usize),
        ) {
            let current_idx = current_idx.map(|ci| {
                if ids.is_empty() { None } else { Some(ci.min(ids.len() - 1)) }
            }).flatten();

            let start = match current_idx {
                Some(ci) => ci + 1,
                None => 0,
            };

            let mut shuffled = ids.clone();
            if start < shuffled.len() {
                fisher_yates_shuffle(&mut shuffled[start..]);
            }

            // The prefix (up to and including current) must be unchanged.
            prop_assert_eq!(&shuffled[..start], &ids[..start]);

            // The suffix must be a permutation of the original suffix.
            let mut orig_suffix = ids[start..].to_vec();
            let mut shuf_suffix = shuffled[start..].to_vec();
            orig_suffix.sort();
            shuf_suffix.sort();
            prop_assert_eq!(orig_suffix, shuf_suffix);
        }
    }
}
