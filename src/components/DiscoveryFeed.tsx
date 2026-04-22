import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore, type Track } from "../store/index";
import "./DiscoveryFeed.css";

// ─── Icons ────────────────────────────────────────────────────────────────────

function IconPlayNext() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
    </svg>
  );
}

function IconSpinner() {
  return (
    <svg
      className="discovery-feed__spinner"
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <circle
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeDasharray="31.4 31.4"
      />
    </svg>
  );
}

// ─── Track Row ────────────────────────────────────────────────────────────────

interface TrackRowProps {
  track: Track;
  onPlayNext: (track: Track) => void;
}

function TrackRow({ track, onPlayNext }: TrackRowProps) {
  const title = track.title ?? track.filename;
  const artist = track.artist ?? "Unknown Artist";

  return (
    <div className="discovery-feed__item">
      <div className="discovery-feed__track-info">
        <span className="discovery-feed__track-title" title={title}>
          {title}
        </span>
        <span className="discovery-feed__track-artist" title={artist}>
          {artist}
        </span>
      </div>
      <button
        className="discovery-feed__play-next-btn"
        onClick={() => onPlayNext(track)}
        aria-label={`Play ${title} next`}
        title="Play Next"
      >
        <IconPlayNext />
      </button>
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function DiscoveryFeed() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const addToast = useMusicStore((s) => s.addToast);

  const [recommendations, setRecommendations] = useState<Track[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (selectedTrackId === null) {
      setRecommendations([]);
      return;
    }

    let cancelled = false;
    setLoading(true);

    invoke<Track[]>("get_recommendations", { trackId: selectedTrackId })
      .then((tracks) => {
        if (!cancelled) {
          setRecommendations(tracks.slice(0, 20));
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.error("Failed to fetch recommendations:", err);
          addToast("Failed to load recommendations");
          setRecommendations([]);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedTrackId]);

  const handlePlayNext = async (track: Track) => {
    try {
      await invoke("queue_add_next", { trackId: track.id });
      await invoke("play_track", { trackId: track.id });
    } catch (err) {
      console.error("Failed to play track:", err);
      addToast("Failed to play track");
    }
  };

  return (
    <div className="discovery-feed">
      <div className="discovery-feed__header">
        <h2 className="discovery-feed__title">Discover</h2>
        {loading && <IconSpinner />}
      </div>

      <div className="discovery-feed__list">
        {loading && recommendations.length === 0 ? (
          <div className="discovery-feed__loading">
            <IconSpinner />
            <span>Finding similar tracks…</span>
          </div>
        ) : recommendations.length === 0 ? (
          <div className="discovery-feed__empty">
            {selectedTrackId === null
              ? "Play a track to get recommendations."
              : "No recommendations found."}
          </div>
        ) : (
          recommendations.map((track) => (
            <TrackRow key={track.id} track={track} onPlayNext={handlePlayNext} />
          ))
        )}
      </div>
    </div>
  );
}
