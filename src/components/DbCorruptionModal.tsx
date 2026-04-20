import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./DbCorruptionModal.css";

export default function DbCorruptionModal() {
  const visible = useMusicStore((s) => s.dbCorruptionVisible);
  const hideDbCorruption = useMusicStore((s) => s.hideDbCorruption);
  const addToast = useMusicStore((s) => s.addToast);
  const [resetting, setResetting] = useState(false);

  if (!visible) return null;

  async function handleReset() {
    setResetting(true);
    try {
      await invoke("reset_library");
      hideDbCorruption();
      addToast("Library has been reset.");
    } catch {
      addToast("Failed to reset library. Please restart the application.");
      hideDbCorruption();
    } finally {
      setResetting(false);
    }
  }

  return (
    <div className="db-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="db-modal-title">
      <div className="db-modal">
        <h2 className="db-modal__title" id="db-modal-title">Library Database Error</h2>
        <p className="db-modal__body">
          The library database could not be read. This may be caused by corruption or an incompatible format.
          Reset to an empty library?
        </p>
        <div className="db-modal__actions">
          <button
            className="db-modal__btn db-modal__btn--cancel"
            onClick={hideDbCorruption}
            disabled={resetting}
          >
            Cancel
          </button>
          <button
            className="db-modal__btn db-modal__btn--reset"
            onClick={handleReset}
            disabled={resetting}
          >
            {resetting ? "Resetting…" : "Reset Library"}
          </button>
        </div>
      </div>
    </div>
  );
}
