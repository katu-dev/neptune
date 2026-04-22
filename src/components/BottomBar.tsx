import { useState, useEffect, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import WaveformView from "./WaveformView";
import VuMeter from "./VuMeter";
import SpectrumVisualizer from "./SpectrumVisualizer";
import OscilloscopeVisualizer from "./OscilloscopeVisualizer";
import "./BottomBar.css";

// ─── Icons ────────────────────────────────────────────────────────────────────

function IconPlay() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>; }
function IconPause() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>; }
function IconSkipPrev() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 6h2v12H6zm3.5 6 8.5 6V6z"/></svg>; }
function IconSkipNext() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 18l8.5-6L6 6v12zm2.5-6 8.5 6V6z"/></svg>; }
function IconVolumeMute() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4 9.91 6.09 12 8.18V4z"/></svg>; }
function IconVolumeLow() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z"/></svg>; }
function IconVolumeHigh() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>; }
function IconShuffle() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z"/></svg>; }
function IconRepeatNone() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z"/></svg>; }
function IconRepeatAll() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z"/></svg>; }
function IconRepeatOne() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zm-4-2v-5h-1l-2 1v1h1.5v3H13z"/></svg>; }
function IconMusicNote() { return <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/></svg>; }

function formatTime(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

// ─── Cover art thumbnail ──────────────────────────────────────────────────────

function CoverThumb({ trackId, coverArtPath }: { trackId: number | null; coverArtPath: string | null }) {
  const [url, setUrl] = useState<string | null>(null);
  const [imgError, setImgError] = useState(false);
  const prevBlobRef = useRef<string | null>(null);

  const fetchEmbedded = (id: number, cancelled: { v: boolean }) => {
    invoke<number[] | null>("get_cover_art", { trackId: id })
      .then((bytes) => {
        if (cancelled.v || !bytes || bytes.length === 0) return;
        const isPng = bytes[0] === 0x89 && bytes[1] === 0x50;
        const blob = new Blob([new Uint8Array(bytes)], { type: isPng ? "image/png" : "image/jpeg" });
        const blobUrl = URL.createObjectURL(blob);
        if (prevBlobRef.current?.startsWith("blob:")) URL.revokeObjectURL(prevBlobRef.current);
        prevBlobRef.current = blobUrl;
        setUrl(blobUrl);
        setImgError(false);
      })
      .catch(() => setUrl(null));
  };

  useEffect(() => {
    if (!trackId) { setUrl(null); setImgError(false); return; }
    setImgError(false);

    const cancelled = { v: false };

    if (coverArtPath) {
      // Try fast path first; onError will fall back to embedded bytes
      setUrl(convertFileSrc(coverArtPath));
    } else {
      fetchEmbedded(trackId, cancelled);
    }

    return () => { cancelled.v = true; };
  }, [trackId, coverArtPath]);

  // Cleanup blob on unmount
  useEffect(() => () => {
    if (prevBlobRef.current?.startsWith("blob:")) URL.revokeObjectURL(prevBlobRef.current);
  }, []);

  const handleImgError = () => {
    if (!trackId || imgError) return;
    setImgError(true);
    const cancelled = { v: false };
    fetchEmbedded(trackId, cancelled);
  };

  if (!url || (imgError && !url?.startsWith("blob:"))) {
    return (
      <div className="bottom-bar__cover-placeholder">
        <IconMusicNote />
      </div>
    );
  }
  return (
    <img
      className="bottom-bar__cover-img"
      src={url}
      alt="Cover art"
      onError={handleImgError}
    />
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function BottomBar() {
  const selectedTrackId  = useMusicStore((s) => s.selectedTrackId);
  const playbackState    = useMusicStore((s) => s.playbackState);
  const playbackPosition = useMusicStore((s) => s.playbackPosition);
  const volume           = useMusicStore((s) => s.volume);
  const setVolume        = useMusicStore((s) => s.setVolume);
  const tracks           = useMusicStore((s) => s.tracks);
  const loopMode         = useMusicStore((s) => s.loopMode);
  const setLoopMode      = useMusicStore((s) => s.setLoopMode);
  const shuffleEnabled   = useMusicStore((s) => s.shuffleEnabled);
  const setShuffleEnabled = useMusicStore((s) => s.setShuffleEnabled);
  const favoriteTrackIds = useMusicStore((s) => s.favoriteTrackIds);
  const toggleFavorite   = useMusicStore((s) => s.toggleFavorite);
  const queueTrackIds    = useMusicStore((s) => s.queueTrackIds);
  const currentQueueIndex = useMusicStore((s) => s.currentQueueIndex);
  const openNowPlaying   = useMusicStore((s) => s.openNowPlaying);

  const playingTrackId =
    currentQueueIndex !== null && queueTrackIds[currentQueueIndex] !== undefined
      ? queueTrackIds[currentQueueIndex]
      : selectedTrackId;
  const currentTrack = tracks.find((t) => t.id === playingTrackId) ?? null;

  const { position_secs, duration_secs } = playbackPosition;
  const isPlaying   = playbackState === "playing";
  const isFavorited = currentTrack ? favoriteTrackIds.has(currentTrack.id) : false;
  const volumePct   = Math.round(volume * 100);

  function cycleLoopMode() {
    if (loopMode === "none") setLoopMode("all");
    else if (loopMode === "all") setLoopMode("one");
    else setLoopMode("none");
  }

  async function handlePlayPause() {
    if (playbackState === "stopped" && selectedTrackId !== null) {
      await invoke("play_track", { trackId: selectedTrackId });
    } else {
      await invoke("pause");
    }
  }

  async function handleVolumeChange(e: React.ChangeEvent<HTMLInputElement>) {
    const level = Number(e.target.value) / 100;
    setVolume(level);
    await invoke("set_volume", { level });
  }

  function VolumeIcon() {
    if (volumePct === 0) return <IconVolumeMute />;
    if (volumePct < 50) return <IconVolumeLow />;
    return <IconVolumeHigh />;
  }

  const loopTitle = loopMode === "none" ? "Loop: off" : loopMode === "all" ? "Loop: all" : "Loop: one";

  return (
    <div className="bottom-bar">
      {/* ── Waveform strip ─────────────────────────────────────────────────── */}
      <div className="bottom-bar__waveform">
        <WaveformView />
      </div>

      {/* ── Controls row ───────────────────────────────────────────────────── */}
      <div className="bottom-bar__controls">

        {/* Left: track info */}
        <div className="bottom-bar__track-info" onClick={openNowPlaying} title="Open Now Playing">
          <div className="bottom-bar__cover">
            <CoverThumb
              trackId={currentTrack?.id ?? null}
              coverArtPath={currentTrack?.cover_art_path ?? null}
            />
          </div>
          {currentTrack ? (
            <div className="bottom-bar__meta">
              <span className="bottom-bar__title">{currentTrack.title ?? currentTrack.filename}</span>
              {currentTrack.artist && (
                <span className="bottom-bar__artist">{currentTrack.artist}</span>
              )}
            </div>
          ) : (
            <span className="bottom-bar__idle">Nothing playing</span>
          )}
          {currentTrack && (
            <button
              className={`bottom-bar__icon-btn${isFavorited ? " bottom-bar__icon-btn--heart" : ""}`}
              onClick={(e) => { e.stopPropagation(); toggleFavorite(currentTrack.id); }}
              aria-label={isFavorited ? "Unfavorite" : "Favorite"}
              title={isFavorited ? "Unfavorite" : "Favorite"}
            >
              {isFavorited ? "♥" : "♡"}
            </button>
          )}
        </div>

        {/* Center: transport */}
        <div className="bottom-bar__transport">
          <button
            className={`bottom-bar__icon-btn${shuffleEnabled ? " bottom-bar__icon-btn--active" : ""}`}
            onClick={() => setShuffleEnabled(!shuffleEnabled)}
            aria-label="Shuffle" title="Shuffle"
          >
            <IconShuffle />
          </button>

          <button
            className="bottom-bar__transport-btn"
            onClick={() => invoke("play_previous")}
            aria-label="Previous"
          >
            <IconSkipPrev />
          </button>

          <button
            className="bottom-bar__play-btn"
            onClick={handlePlayPause}
            aria-label={isPlaying ? "Pause" : "Play"}
          >
            {isPlaying ? <IconPause /> : <IconPlay />}
          </button>

          <button
            className="bottom-bar__transport-btn"
            onClick={() => invoke("play_next")}
            aria-label="Next"
          >
            <IconSkipNext />
          </button>

          <button
            className={`bottom-bar__icon-btn${loopMode !== "none" ? " bottom-bar__icon-btn--active" : ""}`}
            onClick={cycleLoopMode}
            aria-label={loopTitle} title={loopTitle}
          >
            {loopMode === "one" ? <IconRepeatOne /> : loopMode === "all" ? <IconRepeatAll /> : <IconRepeatNone />}
          </button>
        </div>

        {/* Right: time + volume + VU */}
        <div className="bottom-bar__right">
          <span className="bottom-bar__time">
            {formatTime(position_secs)}
            <span className="bottom-bar__time-sep"> / </span>
            {formatTime(duration_secs)}
          </span>

          <div className="bottom-bar__volume">
            <button
              className="bottom-bar__icon-btn"
              onClick={() => { const next = volume > 0 ? 0 : 1; setVolume(next); invoke("set_volume", { level: next }); }}
              aria-label="Toggle mute"
              title={`Volume: ${volumePct}%`}
            >
              <VolumeIcon />
            </button>
            <input
              type="range"
              className="bottom-bar__volume-slider"
              min={0} max={100} step={1}
              value={volumePct}
              onChange={handleVolumeChange}
              style={{ "--fill-pct": `${volumePct}%` } as React.CSSProperties}
              aria-label="Volume"
            />
          </div>

          <div className="bottom-bar__vu">
            <VuMeter />
          </div>

          <div className="bottom-bar__spectrum">
            <SpectrumVisualizer />
          </div>

          <div className="bottom-bar__oscilloscope">
            <OscilloscopeVisualizer />
          </div>
        </div>

      </div>
    </div>
  );
}
