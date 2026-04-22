import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./AmbientBackground.css";

export default function AmbientBackground() {
  const ambientBgEnabled = useMusicStore((s) => s.ambientBgEnabled);
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const tracks = useMusicStore((s) => s.tracks);

  const currentTrack = tracks.find((t) => t.id === selectedTrackId) ?? null;
  const rawUrl = currentTrack?.cover_art_path
    ? convertFileSrc(currentTrack.cover_art_path)
    : null;

  const [currentArtUrl, setCurrentArtUrl] = useState<string | null>(rawUrl);
  const [prevArtUrl, setPrevArtUrl] = useState<string | null>(null);
  const [transitioning, setTransitioning] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (rawUrl === currentArtUrl) return;

    // Start cross-fade: previous becomes the old current
    setPrevArtUrl(currentArtUrl);
    setCurrentArtUrl(rawUrl);
    setTransitioning(true);

    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      setTransitioning(false);
      setPrevArtUrl(null);
    }, 600);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [rawUrl]);

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
