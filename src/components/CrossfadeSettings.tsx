import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { useMusicStore } from "../store/index";
import "./CrossfadeSettings.css";

export default function CrossfadeSettings() {
  const crossfadeSecs = useMusicStore((s) => s.crossfadeSecs);
  const gaplessEnabled = useMusicStore((s) => s.gaplessEnabled);
  const setCrossfadeSecs = useMusicStore((s) => s.setCrossfadeSecs);
  const setGaplessEnabled = useMusicStore((s) => s.setGaplessEnabled);

  // Load persisted settings from backend on mount
  useEffect(() => {
    invoke<{ crossfade_secs: number; gapless_enabled: boolean }>("get_crossfade_settings")
      .then((s) => {
        setCrossfadeSecs(s.crossfade_secs);
        setGaplessEnabled(s.gapless_enabled);
      })
      .catch(() => {});
  }, []);

  function handleGaplessToggle(e: React.ChangeEvent<HTMLInputElement>) {
    const enabled = e.target.checked;
    setGaplessEnabled(enabled);
    invoke("set_gapless_enabled", { enabled }).catch(() => {});
  }

  function handleDurationChange(e: React.ChangeEvent<HTMLInputElement>) {
    const secs = parseFloat(e.target.value);
    setCrossfadeSecs(secs);
    invoke("set_crossfade_duration", { secs }).catch(() => {});
  }

  return (
    <div className="crossfade-settings" aria-label="Crossfade settings">
      <label className="crossfade-settings__toggle">
        <input
          type="checkbox"
          checked={gaplessEnabled}
          onChange={handleGaplessToggle}
          aria-label="Enable gapless playback"
        />
        <span>Gapless playback</span>
      </label>

      <div className={`crossfade-settings__duration${!gaplessEnabled ? " crossfade-settings__duration--disabled" : ""}`}>
        <span className="crossfade-settings__duration-label">
          Crossfade: {crossfadeSecs.toFixed(1)}s
        </span>
        <input
          type="range"
          className="crossfade-settings__slider"
          min="0.5"
          max="10"
          step="0.5"
          value={crossfadeSecs}
          onChange={handleDurationChange}
          disabled={!gaplessEnabled}
          aria-label="Crossfade duration"
          aria-valuemin={0.5}
          aria-valuemax={10}
          aria-valuenow={crossfadeSecs}
        />
      </div>
    </div>
  );
}
