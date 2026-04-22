import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useMusicStore } from "../store/index";
import EqualizerPanel from "./EqualizerPanel";
import CrossfadeSettings from "./CrossfadeSettings";
import KeybindSettings from "./KeybindSettings";
import PannerControl from "./PannerControl";
import "./SettingsPanel.css";

interface Props {
  onClose: () => void;
}

export default function SettingsPanel({ onClose }: Props) {
  const { volume, setVolume, addToast, showDbCorruption, bumpTreeVersion,
    ambientBgEnabled, setAmbientBgEnabled,
    discordPresenceEnabled, setDiscordPresenceEnabled,
  } = useMusicStore();

  async function handleToggleAmbientBg() {
    const next = !ambientBgEnabled;
    setAmbientBgEnabled(next);
    // ambient bg is frontend-only state; no backend command needed
  }

  async function handleToggleDiscord() {
    const next = !discordPresenceEnabled;
    setDiscordPresenceEnabled(next);
    invoke("set_discord_enabled", { enabled: next }).catch(() => {});
  }

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
            <div className="settings-section__label">Visual</div>
            <div className="settings-row">
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <div>
                  <span className="settings-row__title">Ambient Background</span>
                  <span className="settings-row__desc">Blurs the current track's cover art as the app background.</span>
                </div>
                <input
                  type="checkbox"
                  checked={ambientBgEnabled}
                  onChange={handleToggleAmbientBg}
                  aria-label="Toggle ambient background"
                  style={{ width: 18, height: 18, cursor: "pointer", flexShrink: 0 }}
                />
              </div>
            </div>
          </section>

          <section>
            <div className="settings-section__label">Integrations</div>
            <div className="settings-row">
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <div>
                  <span className="settings-row__title">Discord Rich Presence</span>
                  <span className="settings-row__desc">Show currently playing track in your Discord status.</span>
                </div>
                <input
                  type="checkbox"
                  checked={discordPresenceEnabled}
                  onChange={handleToggleDiscord}
                  aria-label="Toggle Discord Rich Presence"
                  style={{ width: 18, height: 18, cursor: "pointer", flexShrink: 0 }}
                />
              </div>
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
            <div className="settings-section__label">Panning</div>
            <PannerControl />
          </section>

          <section>
            <div className="settings-section__label">Equalizer</div>
            <EqualizerPanel />
          </section>

          <section>
            <div className="settings-section__label">Crossfade</div>
            <CrossfadeSettings />
          </section>

          <section>
            <div className="settings-section__label">Keybinds</div>
            <KeybindSettings />
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
