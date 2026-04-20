import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useMusicStore } from "../store/index";
import "./SettingsPanel.css";

interface Props {
  onClose: () => void;
}

export default function SettingsPanel({ onClose }: Props) {
  const { volume, setVolume, addToast, showDbCorruption, bumpTreeVersion } = useMusicStore();

  async function handleAddFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Select Music Folder" });
      if (!selected) return;
      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;
      await invoke("scan_directory", { path });
      bumpTreeVersion();
      addToast("Folder scanned successfully.");
    } catch (err) {
      addToast(`Failed to scan folder: ${err}`);
    }
  }

  async function handleRescan() {
    try {
      // Re-scan all known root directories from persisted state
      const state = await invoke<{ root_directories: string[] }>("get_app_state");
      if (!state.root_directories.length) {
        addToast("No folders added yet. Use 'Add Folder' first.");
        return;
      }
      for (const dir of state.root_directories) {
        await invoke("scan_directory", { path: dir });
      }
      bumpTreeVersion();
      addToast("Rescan complete.");
    } catch (err) {
      addToast(`Rescan failed: ${err}`);
    }
  }

  async function handleResetLibrary() {
    try {
      await invoke("reset_library");
      bumpTreeVersion();
      addToast("Library reset.");
    } catch (err) {
      const e = err as { type?: string };
      if (e?.type === "Database") {
        showDbCorruption();
      } else {
        addToast(`Reset failed: ${err}`);
      }
    }
  }

  function handleVolumeChange(e: React.ChangeEvent<HTMLInputElement>) {
    const level = parseFloat(e.target.value);
    setVolume(level);
    invoke("set_volume", { level }).catch(() => {});
  }

  return (
    <div className="settings-overlay" role="dialog" aria-modal="true" aria-label="Settings">
      <div className="settings-overlay__backdrop" onClick={onClose} />
      <div className="settings-panel">
        <div className="settings-panel__header">
          <span className="settings-panel__title">Settings</span>
          <button className="settings-panel__close" onClick={onClose} aria-label="Close settings">✕</button>
        </div>
        <div className="settings-panel__body">

          <section>
            <div className="settings-section__label">Playback</div>
            <div className="settings-row">
              <span className="settings-row__title">Default volume: {Math.round(volume * 100)}%</span>
              <span className="settings-row__desc">Adjusts the current and default playback volume.</span>
              <input
                type="range"
                className="settings-row__slider"
                min={0}
                max={1}
                step={0.01}
                value={volume}
                onChange={handleVolumeChange}
                aria-label="Volume"
              />
            </div>
          </section>

          <section>
            <div className="settings-section__label">Library</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <button className="settings-btn" onClick={handleAddFolder}>
                + Add Music Folder
              </button>
              <button className="settings-btn" onClick={handleRescan}>
                ↺ Rescan All Folders
              </button>
            </div>
          </section>

          <section>
            <div className="settings-section__label">Danger Zone</div>
            <button className="settings-btn settings-btn--danger" onClick={handleResetLibrary}>
              Reset Library
            </button>
            <div className="settings-row__desc" style={{ marginTop: 6 }}>
              Removes all indexed tracks. Your files are not deleted.
            </div>
          </section>

        </div>
      </div>
    </div>
  );
}
