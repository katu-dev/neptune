import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useDroppable } from "@dnd-kit/core";
import { useMusicStore } from "../store/index";
import AmbientBackground from "./AmbientBackground";
import PlaybackControls from "./PlaybackControls";
import "./NowPlayingView.css";

// ─── Icons ────────────────────────────────────────────────────────────────────

function IconClose() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
    </svg>
  );
}

function IconMusicNote() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z" />
    </svg>
  );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatDuration(secs: number | null): string {
  if (secs === null || secs === undefined) return "—";
  const total = Math.max(0, Math.floor(secs));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

// ─── Component ────────────────────────────────────────────────────────────────

export default function NowPlayingView() {
  const nowPlayingOpen = useMusicStore((s) => s.nowPlayingOpen);
  const closeNowPlaying = useMusicStore((s) => s.closeNowPlaying);
  const ambientBgEnabled = useMusicStore((s) => s.ambientBgEnabled);
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const queueTrackIds = useMusicStore((s) => s.queueTrackIds);
  const currentQueueIndex = useMusicStore((s) => s.currentQueueIndex);
  const tracks = useMusicStore((s) => s.tracks);

  const playingTrackId =
    currentQueueIndex !== null && queueTrackIds[currentQueueIndex] !== undefined
      ? queueTrackIds[currentQueueIndex]
      : selectedTrackId;

  const currentTrack = tracks.find((t) => t.id === playingTrackId) ?? null;

  const { setNodeRef, isOver } = useDroppable({ id: "now-playing-drop" });

  const [coverArtUrl, setCoverArtUrl] = useState<string | null>(null);
  const prevBlobRef = useRef<string | null>(null);

  const revokePrev = () => {
    if (prevBlobRef.current?.startsWith("blob:")) {
      URL.revokeObjectURL(prevBlobRef.current);
      prevBlobRef.current = null;
    }
  };

  useEffect(() => {
    if (!currentTrack) {
      revokePrev();
      setCoverArtUrl(null);
      return;
    }

    const cancelled = { v: false };
    invoke<number[] | null>("get_cover_art", { trackId: currentTrack.id })
      .then((bytes) => {
        if (cancelled.v || !bytes || bytes.length === 0) return;
        const isPng = bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
        const blob = new Blob([new Uint8Array(bytes)], { type: isPng ? "image/png" : "image/jpeg" });
        const url = URL.createObjectURL(blob);
        revokePrev();
        prevBlobRef.current = url;
        setCoverArtUrl(url);
      })
      .catch(() => setCoverArtUrl(null));

    return () => { cancelled.v = true; };
  }, [currentTrack?.id]);

  useEffect(() => () => revokePrev(), []);

  // Close on Escape key
  useEffect(() => {
    if (!nowPlayingOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeNowPlaying();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [nowPlayingOpen, closeNowPlaying]);

  if (!nowPlayingOpen) return null;

  const title = currentTrack?.title || "No Track Playing";
  const artist = currentTrack?.artist || "";
  const album = currentTrack?.album || "";
  const duration = currentTrack?.duration_secs ?? null;

  return (
    <div
      className={`now-playing-overlay${isOver ? " now-playing-overlay--drop-active" : ""}`}
      ref={setNodeRef}
      role="dialog"
      aria-label="Now Playing"
      aria-modal="true"
    >
      {/* Ambient background layer */}
      {ambientBgEnabled && <AmbientBackground />}

      {/* Close button */}
      <button
        className="now-playing-overlay__close-btn"
        onClick={closeNowPlaying}
        aria-label="Close Now Playing view"
        title="Close (Esc)"
      >
        <IconClose />
      </button>

      {/* Main content */}
      <div className="now-playing-overlay__content">
        {/* Cover art */}
        <div className="now-playing-overlay__cover-art">
          {coverArtUrl ? (
            <img
              src={coverArtUrl}
              alt={`Cover art for ${title}`}
              className="now-playing-overlay__cover-img"
            />
          ) : (
            <div className="now-playing-overlay__cover-placeholder">
              <IconMusicNote />
            </div>
          )}
        </div>

        {/* Track metadata */}
        <div className="now-playing-overlay__metadata">
          <h1 className="now-playing-overlay__title">{title}</h1>
          {artist && <p className="now-playing-overlay__artist">{artist}</p>}
          {album && <p className="now-playing-overlay__album">{album}</p>}
          {duration !== null && (
            <p className="now-playing-overlay__duration">{formatDuration(duration)}</p>
          )}
        </div>

        {/* Playback controls */}
        <div className="now-playing-overlay__controls">
          <PlaybackControls />
        </div>
      </div>

      {/* Drop indicator */}
      {isOver && (
        <div className="now-playing-overlay__drop-indicator">
          Drop to play next
        </div>
      )}
    </div>
  );
}
