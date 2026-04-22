import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./PlaybackControls.css";

function formatTime(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function IconPlay() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>; }
function IconPause() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" /></svg>; }
function IconStop() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 6h12v12H6z" /></svg>; }
function IconSkipPrev() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 6h2v12H6zm3.5 6 8.5 6V6z" /></svg>; }
function IconSkipNext() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 18l8.5-6L6 6v12zm2.5-6 8.5 6V6z" /></svg>; }
function IconVolume() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z" /></svg>; }
function IconShuffle() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z" /></svg>; }
function IconRepeatNone() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z" /></svg>; }
function IconRepeatAll() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z" /></svg>; }
function IconRepeatOne() { return <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zm-4-2v-5h-1l-2 1v1h1.5v3H13z" /></svg>; }

export default function PlaybackControls() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const playbackState = useMusicStore((s) => s.playbackState);
  const playbackPosition = useMusicStore((s) => s.playbackPosition);
  const volume = useMusicStore((s) => s.volume);
  const setVolume = useMusicStore((s) => s.setVolume);
  const tracks = useMusicStore((s) => s.tracks);
  const loopMode = useMusicStore((s) => s.loopMode);
  const setLoopMode = useMusicStore((s) => s.setLoopMode);
  const shuffleEnabled = useMusicStore((s) => s.shuffleEnabled);
  const setShuffleEnabled = useMusicStore((s) => s.setShuffleEnabled);
  const favoriteTrackIds = useMusicStore((s) => s.favoriteTrackIds);
  const toggleFavorite = useMusicStore((s) => s.toggleFavorite);
  const queueTrackIds = useMusicStore((s) => s.queueTrackIds);
  const currentQueueIndex = useMusicStore((s) => s.currentQueueIndex);

  const playingTrackId =
    currentQueueIndex !== null && queueTrackIds[currentQueueIndex] !== undefined
      ? queueTrackIds[currentQueueIndex]
      : selectedTrackId;
  const currentTrack = tracks.find((t) => t.id === playingTrackId) ?? null;

  const { position_secs, duration_secs } = playbackPosition;
  const [seekValue, setSeekValue] = useState<number | null>(null);
  const displayPosition = seekValue !== null ? seekValue : position_secs;
  const sliderMax = duration_secs > 0 ? duration_secs : 1;

  async function handlePlayPause() {
    if (playbackState === "stopped" && selectedTrackId !== null) {
      await invoke("play_track", { trackId: selectedTrackId });
    } else {
      await invoke("pause");
    }
  }

  function cycleLoopMode() {
    if (loopMode === "none") setLoopMode("all");
    else if (loopMode === "all") setLoopMode("one");
    else setLoopMode("none");
  }

  function handleSeekChange(e: React.ChangeEvent<HTMLInputElement>) {
    setSeekValue(Number(e.target.value));
  }

  async function handleSeekCommit() {
    const value = seekValue ?? position_secs;
    setSeekValue(null);
    await invoke("seek", { positionSecs: value });
  }

  async function handleVolumeChange(e: React.ChangeEvent<HTMLInputElement>) {
    const level = Number(e.target.value) / 100;
    setVolume(level);
    await invoke("set_volume", { level });
  }

  const isPlaying = playbackState === "playing";
  const seekPercent = duration_secs > 0 ? (displayPosition / duration_secs) * 100 : 0;
  const volumePct = Math.round(volume * 100);
  const isFavorited = currentTrack ? favoriteTrackIds.has(currentTrack.id) : false;

  return (
    <div className="playback-controls" role="region" aria-label="Playback controls">
      {/* Mini now-playing info */}
      <div className="playback-controls__now-playing">
        {currentTrack ? (
          <>
            <div className="playback-controls__now-playing-text">
              <span className="playback-controls__now-playing-title">
                {currentTrack.title ?? currentTrack.filename}
              </span>
              {currentTrack.artist && (
                <span className="playback-controls__now-playing-artist">{currentTrack.artist}</span>
              )}
            </div>
            <button
              className={`playback-controls__btn playback-controls__btn--icon${isFavorited ? " playback-controls__btn--active-heart" : ""}`}
              onClick={() => toggleFavorite(currentTrack.id)}
              aria-label={isFavorited ? "Remove from favorites" : "Add to favorites"}
              title={isFavorited ? "Unfavorite" : "Favorite"}
            >
              {isFavorited ? "♥" : "♡"}
            </button>
          </>
        ) : (
          <span className="playback-controls__now-playing-empty">No track playing</span>
        )}
      </div>

      {/* Center: transport + seek */}
      <div className="playback-controls__center">
        <div className="playback-controls__transport">
          <button
            className={`playback-controls__btn playback-controls__btn--icon${shuffleEnabled ? " playback-controls__btn--active" : ""}`}
            onClick={() => setShuffleEnabled(!shuffleEnabled)}
            aria-label="Shuffle" title="Shuffle"
          >
            <IconShuffle />
          </button>
          <button className="playback-controls__btn playback-controls__btn--secondary" onClick={() => invoke("play_previous")} aria-label="Previous" title="Previous">
            <IconSkipPrev />
          </button>
          <button className="playback-controls__btn playback-controls__btn--primary" onClick={handlePlayPause} aria-label={isPlaying ? "Pause" : "Play"} title={isPlaying ? "Pause" : "Play"}>
            {isPlaying ? <IconPause /> : <IconPlay />}
          </button>
          <button className="playback-controls__btn playback-controls__btn--secondary" onClick={() => invoke("stop")} aria-label="Stop" title="Stop">
            <IconStop />
          </button>
          <button className="playback-controls__btn playback-controls__btn--secondary" onClick={() => invoke("play_next")} aria-label="Next" title="Next">
            <IconSkipNext />
          </button>
          <button
            className={`playback-controls__btn playback-controls__btn--icon${loopMode !== "none" ? " playback-controls__btn--active" : ""}`}
            onClick={cycleLoopMode}
            aria-label={`Loop: ${loopMode}`}
            title={loopMode === "none" ? "Loop off" : loopMode === "all" ? "Loop all" : "Loop one"}
          >
            {loopMode === "one" ? <IconRepeatOne /> : loopMode === "all" ? <IconRepeatAll /> : <IconRepeatNone />}
          </button>
        </div>

        <div className="playback-controls__seek-area">
          <span className="playback-controls__time">{formatTime(displayPosition)}</span>
          <div className="playback-controls__slider-wrap">
            <input
              type="range"
              className="playback-controls__slider playback-controls__slider--seek"
              min={0} max={sliderMax} step={0.1}
              value={displayPosition}
              onChange={handleSeekChange}
              onMouseUp={handleSeekCommit}
              onKeyUp={handleSeekCommit}
              style={{ "--fill-pct": `${seekPercent}%` } as React.CSSProperties}
              aria-label="Seek"
            />
          </div>
          <span className="playback-controls__time">{formatTime(duration_secs)}</span>
        </div>
      </div>

      {/* Volume */}
      <div className="playback-controls__volume">
        <IconVolume />
        <input
          type="range"
          className="playback-controls__slider playback-controls__slider--volume"
          min={0} max={100} step={1}
          value={volumePct}
          onChange={handleVolumeChange}
          style={{ "--fill-pct": `${volumePct}%` } as React.CSSProperties}
          aria-label="Volume"
        />
        <span className="playback-controls__volume-label">{volumePct}%</span>
      </div>
    </div>
  );
}
