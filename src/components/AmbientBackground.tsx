import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./AmbientBackground.css";

export default function AmbientBackground() {
  const ambientBgEnabled = useMusicStore((s) => s.ambientBgEnabled);
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const tracks = useMusicStore((s) => s.tracks);

  const currentTrack = tracks.find((t) => t.id === selectedTrackId) ?? null;

  const [currentArtUrl, setCurrentArtUrl] = useState<string | null>(null);
  const [prevArtUrl, setPrevArtUrl] = useState<string | null>(null);
  const [transitioning, setTransitioning] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!currentTrack) {
      setCurrentArtUrl(null);
      return;
    }

    let cancelled = false;

    const resolve = async () => {
      let url: string | null = null;

      // Always fetch via get_cover_art — the Rust command handles both the
      // cached-file fast path and the embedded-bytes slow path, so we get
      // a reliable result regardless of whether cover_art_path is stale.
      try {
        const bytes = await invoke<number[] | null>("get_cover_art", { trackId: currentTrack.id });
        if (bytes && bytes.length > 0) {
          const isPng = bytes[0] === 0x89 && bytes[1] === 0x50;
          const blob = new Blob([new Uint8Array(bytes)], { type: isPng ? "image/png" : "image/jpeg" });
          url = URL.createObjectURL(blob);
        }
      } catch {
        url = null;
      }

      if (cancelled) return;

      setCurrentArtUrl((prev) => {
        if (prev?.startsWith("blob:")) URL.revokeObjectURL(prev);
        if (prev === url) return prev;

        setPrevArtUrl(prev);
        setTransitioning(true);

        if (timerRef.current) clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => {
          setTransitioning(false);
          setPrevArtUrl(null);
        }, 600);

        return url;
      });
    };

    resolve();

    return () => {
      cancelled = true;
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [currentTrack?.id, currentTrack?.cover_art_path]);

  if (!ambientBgEnabled) return null;

  return (
    <div className="ambient-bg">
      {prevArtUrl ? (
        <img
          src={prevArtUrl}
          className={`ambient-img${transitioning ? " ambient-img--fade-out" : ""}`}
          aria-hidden="true"
          alt=""
        />
      ) : (
        !currentArtUrl && <div className="ambient-bg__fallback" aria-hidden="true" />
      )}
      {currentArtUrl ? (
        <img
          src={currentArtUrl}
          className={`ambient-img${transitioning ? " ambient-img--fade-in" : " ambient-img--visible"}`}
          aria-hidden="true"
          alt=""
        />
      ) : (
        <div className="ambient-bg__fallback" aria-hidden="true" />
      )}
    </div>
  );
}
