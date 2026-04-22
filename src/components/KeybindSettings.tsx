import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./KeybindSettings.css";

interface ConflictError {
  type: "Conflict";
  action: string;
  combo: string;
}

const ACTION_LABELS: Record<string, string> = {
  play_pause: "Play / Pause",
  next_track: "Next Track",
  prev_track: "Previous Track",
  volume_up: "Volume Up",
  volume_down: "Volume Down",
  seek_forward: "Seek Forward",
  seek_backward: "Seek Backward",
  command_palette: "Command Palette",
};

function comboFromEvent(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Meta");
  // Avoid adding modifier-only presses as the key
  if (!["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
    parts.push(e.code);
  }
  return parts.join("+");
}

export default function KeybindSettings() {
  const keybindMap = useMusicStore((s) => s.keybindMap);
  const setKeybindMap = useMusicStore((s) => s.setKeybindMap);

  const [recordingAction, setRecordingAction] = useState<string | null>(null);
  const [conflict, setConflict] = useState<{ action: string; combo: string; conflictingAction: string } | null>(null);

  // Load keybinds from backend on mount
  useEffect(() => {
    invoke<Record<string, string>>("get_keybinds")
      .then((map) => setKeybindMap(map))
      .catch(() => {});
  }, [setKeybindMap]);

  const handleKeyDown = useCallback(
    async (e: KeyboardEvent) => {
      if (!recordingAction) return;
      e.preventDefault();
      e.stopPropagation();

      const combo = comboFromEvent(e);
      // Ignore bare modifier presses
      if (!combo || combo === "" || ["Ctrl", "Alt", "Shift", "Meta"].includes(combo)) return;

      setRecordingAction(null);

      try {
        await invoke("set_keybind", { action: recordingAction, combo });
        const updated = await invoke<Record<string, string>>("get_keybinds");
        setKeybindMap(updated);
      } catch (err) {
        const e = err as ConflictError;
        if (e?.type === "Conflict") {
          // Find which action currently owns this combo
          const conflictingAction =
            Object.entries(keybindMap).find(([, v]) => v === combo)?.[0] ?? e.action;
          setConflict({ action: recordingAction, combo, conflictingAction });
        }
      }
    },
    [recordingAction, keybindMap, setKeybindMap]
  );

  useEffect(() => {
    if (recordingAction) {
      window.addEventListener("keydown", handleKeyDown, true);
    }
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [recordingAction, handleKeyDown]);

  async function handleConflictConfirm() {
    if (!conflict) return;
    const { action, combo } = conflict;
    setConflict(null);
    try {
      // Force-overwrite: first clear the conflicting action's binding by
      // assigning it an empty combo, then set the new binding.
      const conflictingAction = Object.entries(keybindMap).find(([, v]) => v === combo)?.[0];
      if (conflictingAction) {
        // Unbind the conflicting action by setting it to a unique placeholder,
        // then immediately rebind the desired action.
        await invoke("set_keybind", { action: conflictingAction, combo: `__unbound_${Date.now()}` });
      }
      await invoke("set_keybind", { action, combo });
      const updated = await invoke<Record<string, string>>("get_keybinds");
      setKeybindMap(updated);
    } catch {
      // If still fails, silently ignore
    }
  }

  function handleConflictCancel() {
    setConflict(null);
  }

  async function handleResetDefaults() {
    try {
      await invoke("reset_keybinds");
      const updated = await invoke<Record<string, string>>("get_keybinds");
      setKeybindMap(updated);
    } catch {
      // ignore
    }
  }

  const actions = Object.keys(ACTION_LABELS);

  return (
    <div className="keybind-settings" aria-label="Keybind settings">
      <div className="keybind-settings__list">
        {actions.map((action) => {
          const isRecording = recordingAction === action;
          const combo = keybindMap[action] ?? "—";
          return (
            <div key={action} className="keybind-row">
              <span className="keybind-row__action">{ACTION_LABELS[action]}</span>
              <kbd className={`keybind-row__combo${isRecording ? " keybind-row__combo--recording" : ""}`}>
                {isRecording ? "Press a key…" : combo}
              </kbd>
              <button
                className={`keybind-row__record-btn${isRecording ? " keybind-row__record-btn--active" : ""}`}
                onClick={() => setRecordingAction(isRecording ? null : action)}
                aria-label={isRecording ? `Cancel recording for ${ACTION_LABELS[action]}` : `Record keybind for ${ACTION_LABELS[action]}`}
                aria-pressed={isRecording}
              >
                {isRecording ? "Cancel" : "Record"}
              </button>
            </div>
          );
        })}
      </div>

      <button
        className="settings-btn keybind-settings__reset-btn"
        onClick={handleResetDefaults}
        aria-label="Reset keybinds to defaults"
      >
        Reset to Defaults
      </button>

      {conflict && (
        <div className="keybind-conflict-overlay" role="dialog" aria-modal="true" aria-label="Keybind conflict">
          <div className="keybind-conflict-dialog">
            <p className="keybind-conflict-dialog__message">
              <strong>{conflict.combo}</strong> is already bound to{" "}
              <strong>{ACTION_LABELS[conflict.conflictingAction] ?? conflict.conflictingAction}</strong>.
              Reassign it to <strong>{ACTION_LABELS[conflict.action]}</strong>?
            </p>
            <div className="keybind-conflict-dialog__actions">
              <button
                className="settings-btn"
                onClick={handleConflictConfirm}
                aria-label="Confirm reassignment"
              >
                Confirm
              </button>
              <button
                className="settings-btn keybind-conflict-dialog__cancel"
                onClick={handleConflictCancel}
                aria-label="Cancel reassignment"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
