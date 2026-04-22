import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./PlaybackControls.css";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatTime(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

// ─── Icons ────────────────────────────────────────────────────────────────────

function IconPlay() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M8 5v14l11-7z" />
    </svg>
  );
}

function IconPause() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
    </svg>
  );
}

function IconStop() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 6h12v12H6z" />
    </svg>
  );
}

function IconSkipPrev() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 6h2v12H6zm3.5 6 8.5 6V6z" />
    </svg>
  );
}

function IconSkipNext() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 18l8.5-6L6 6v12zm2.5-6 8.5 6V6z" />
    </svg>
  );
}

function IconVolume() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z" />
    </svg>
  );
}

// ─── Component ────────────────────────────────────────────────────────────────

export default function PlaybackControls() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const playbackState = useMusicStore((s) => s.playbackState);
  const playbackPosition = useMusicStore((s) => s.playbackPosition);
  const volume = useMusicStore((s) => s.volume);
  const setVolume = useMusicStore((s) => s.setVolume);

  const { position_secs, duration_secs } = playbackPosition;

  // Local seek value while dragging — avoids spamming seek commands
  const [seekValue, setSeekValue] = useState<number | null>(null);

  const displayPosition = seekValue !== null ? seekValue : position_secs;
  const sliderMax = duration_secs > 0 ? duration_secs : 1;

  // ── Playback actions ──────────────────────────────────────────────────────

  async function handlePlayPause() {
    // Backend `pause` command toggles: playing→pause, paused→resume, stopped→no-op
    if (playbackState === "stopped" && selectedTrackId !== null) {
      await invoke("play_track", { trackId: selectedTrackId });
    } else {
      await invoke("pause");
    }
  }

  async function handleStop() {
    await invoke("stop");
  }

  async function handlePrevious() {
    await invoke("play_previous");
  }

  async function handleNext() {
    await invoke("play_next");
  }

  // ── Seek slider ───────────────────────────────────────────────────────────

  function handleSeekChange(e: React.ChangeEvent<HTMLInputElement>) {
    setSeekValue(Number(e.target.value));
  }

  async function handleSeekCommit(e: React.MouseEvent<HTMLInputElement> | React.KeyboardEvent<HTMLInputElement>) {
    const value = seekValue ?? position_secs;
    setSeekValue(null);
    await invoke("seek", { positionSecs: value });
    void e;
  }

  // ── Volume slider ─────────────────────────────────────────────────────────

  async function handleVolumeChange(e: React.ChangeEvent<HTMLInputElement>) {
    const pct = Number(e.target.value); // 0–100
    const level = pct / 100;
    setVolume(level);
    await invoke("set_volume", { level });
  }

  // ── Derived ───────────────────────────────────────────────────────────────

  const isPlaying = playbackState === "playing";
  const seekPercent = duration_secs > 0 ? (displayPosition / duration_secs) * 100 : 0;
  const volumePct = Math.round(volume * 100);

  return (
    <div className="playback-controls" role="region" aria-label="Playback controls">
      {/* Transport buttons */}
      <div className="playback-controls__transport">
        <button
          className="playback-controls__btn playback-controls__btn--secondary"
          onClick={handlePrevious}
          aria-label="Previous track"
          title="Previous"
        >
          <IconSkipPrev />
        </button>

        <button
          className="playback-controls__btn playback-controls__btn--primary"
          onClick={handlePlayPause}
          aria-label={isPlaying ? "Pause" : "Play"}
          title={isPlaying ? "Pause" : "Play"}
        >
          {isPlaying ? <IconPause /> : <IconPlay />}
        </button>

        <button
          className="playback-controls__btn playback-controls__btn--secondary"
          onClick={handleStop}
          aria-label="Stop"
          title="Stop"
        >
          <IconStop />
        </button>

        <button
          className="playback-controls__btn playback-controls__btn--secondary"
          onClick={handleNext}
          aria-label="Next track"
          title="Next"
        >
          <IconSkipNext />
        </button>
      </div>

      {/* Seek area */}
      <div className="playback-controls__seek-area">
        <span className="playback-controls__time" aria-label="Current position">
          {formatTime(displayPosition)}
        </span>

        <div className="playback-controls__slider-wrap">
          <input
            type="range"
            className="playback-controls__slider playback-controls__slider--seek"
            min={0}
            max={sliderMax}
            step={0.1}
            value={displayPosition}
            onChange={handleSeekChange}
            onMouseUp={handleSeekCommit}
            onKeyUp={handleSeekCommit}
            style={{ "--fill-pct": `${seekPercent}%` } as React.CSSProperties}
            aria-label="Seek"
            aria-valuemin={0}
            aria-valuemax={sliderMax}
            aria-valuenow={displayPosition}
            aria-valuetext={formatTime(displayPosition)}
          />
        </div>

        <span className="playback-controls__time" aria-label="Duration">
          {formatTime(duration_secs)}
        </span>
      </div>

      {/* Volume */}
      <div className="playback-controls__volume">
        <IconVolume />
        <input
          type="range"
          className="playback-controls__slider playback-controls__slider--volume"
          min={0}
          max={100}
          step={1}
          value={volumePct}
          onChange={handleVolumeChange}
          style={{ "--fill-pct": `${volumePct}%` } as React.CSSProperties}
          aria-label="Volume"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={volumePct}
        />
      </div>
    </div>
  );
}
