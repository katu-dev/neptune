import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore, type Track } from "../store/index";
import "./MetadataPanel.css";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatDuration(secs: number | null): string | null {
  if (secs === null) return null;
  const total = Math.round(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

function bytesToDataUrl(bytes: number[]): string {
  // Convert raw byte array from Tauri to a base64 data URL.
  // We don't know the mime type, so we try image/jpeg as the most common
  // embedded cover art format; browsers will still render png/webp correctly.
  const binary = String.fromCharCode(...bytes);
  const b64 = btoa(binary);
  return `data:image/jpeg;base64,${b64}`;
}

// ─── Field row ────────────────────────────────────────────────────────────────

interface FieldProps {
  label: string;
  value: string | number | null | undefined;
  mono?: boolean;
}

function Field({ label, value, mono }: FieldProps) {
  const isEmpty = value === null || value === undefined || value === "";
  return (
    <div className="metadata-panel__field">
      <span className="metadata-panel__label">{label}</span>
      <span
        className={[
          "metadata-panel__value",
          isEmpty ? "metadata-panel__value--null" : "",
          mono ? "metadata-panel__value--mono" : "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        {isEmpty ? "—" : String(value)}
      </span>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function MetadataPanel() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const tracks = useMusicStore((s) => s.tracks);
  const { tags, trackTagMap, setTrackTagMap, addToast } = useMusicStore();

  const [track, setTrack] = useState<Track | null>(null);
  const [coverUrl, setCoverUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Keep a ref to the latest requested id so stale responses are discarded.
  const latestIdRef = useRef<number | null>(null);

  // Reload trackTagMap from backend
  async function reloadTrackTags() {
    const pairs = await invoke<[number, number][]>("get_all_track_tags");
    setTrackTagMap(pairs);
  }

  async function handleToggleTag(tagId: number) {
    if (!selectedTrackId) return;
    const assigned = trackTagMap.get(selectedTrackId) ?? [];
    try {
      if (assigned.includes(tagId)) {
        await invoke("remove_tag_from_track", { trackId: selectedTrackId, tagId });
      } else {
        await invoke("assign_tag", { trackId: selectedTrackId, tagId });
      }
      await reloadTrackTags();
    } catch (err) {
      addToast(`Failed to update tag: ${err}`);
    }
  }

  useEffect(() => {
    if (selectedTrackId === null) {
      setTrack(null);
      setCoverUrl(null);
      return;
    }

    latestIdRef.current = selectedTrackId;
    setLoading(true);

    // Optimistically show data already in the store while the full fetch runs.
    const storeTrack = tracks.find((t) => t.id === selectedTrackId) ?? null;
    if (storeTrack) setTrack(storeTrack);

    const id = selectedTrackId;

    Promise.all([
      invoke<Track>("get_track_metadata", { trackId: id }),
      invoke<number[] | null>("get_cover_art", { trackId: id }),
    ])
      .then(([metadata, coverBytes]) => {
        if (latestIdRef.current !== id) return; // stale response
        setTrack(metadata);
        setCoverUrl(coverBytes && coverBytes.length > 0 ? bytesToDataUrl(coverBytes) : null);
      })
      .catch(() => {
        if (latestIdRef.current !== id) return;
        // Keep whatever we already have from the store
      })
      .finally(() => {
        if (latestIdRef.current === id) setLoading(false);
      });
  }, [selectedTrackId, tracks]);

  if (selectedTrackId === null) {
    return (
      <div className="metadata-panel">
        <div className="metadata-panel__empty">Select a track to view its metadata.</div>
      </div>
    );
  }

  if (loading && !track) {
    return (
      <div className="metadata-panel">
        <div className="metadata-panel__empty">Loading…</div>
      </div>
    );
  }

  if (!track) {
    return (
      <div className="metadata-panel">
        <div className="metadata-panel__empty">No metadata available.</div>
      </div>
    );
  }

  return (
    <div className="metadata-panel" aria-label="Track metadata">
      {/* Cover art */}
      <div className="metadata-panel__cover" aria-label="Cover art">
        {coverUrl ? (
          <img
            className="metadata-panel__cover-img"
            src={coverUrl}
            alt={track.album ?? "Album cover"}
          />
        ) : (
          <span className="metadata-panel__cover-placeholder" aria-hidden="true">
            🎵
          </span>
        )}
      </div>

      {/* Tag fields */}
      <div className="metadata-panel__fields">
        <Field label="Title" value={track.title} />
        <Field label="Artist" value={track.artist} />
        <Field label="Album" value={track.album} />
        <Field label="Album Artist" value={track.album_artist} />
        <Field label="Year" value={track.year} />
        <Field label="Genre" value={track.genre} />
        <Field label="Track" value={track.track_number} />
        <Field label="Disc" value={track.disc_number} />
        <Field
          label="Duration"
          value={formatDuration(track.duration_secs)}
        />

        <div className="metadata-panel__divider" />

        {/* Tags assignment */}
        {tags.length > 0 && (
          <>
            <div className="metadata-panel__label">Tags</div>
            <div className="metadata-panel__tags">
              {tags.map((tag) => {
                const assigned = (trackTagMap.get(selectedTrackId!) ?? []).includes(tag.id);
                return (
                  <button
                    key={tag.id}
                    className={`metadata-panel__tag-pill${assigned ? " metadata-panel__tag-pill--active" : ""}`}
                    style={assigned ? { background: tag.color, borderColor: tag.color, color: "#fff" } : { borderColor: tag.color, color: tag.color }}
                    onClick={() => handleToggleTag(tag.id)}
                    title={assigned ? `Remove tag "${tag.name}"` : `Add tag "${tag.name}"`}
                  >
                    <span className="metadata-panel__tag-dot" style={{ background: assigned ? "#fff" : tag.color }} />
                    {tag.name}
                  </button>
                );
              })}
            </div>
            <div className="metadata-panel__divider" />
          </>
        )}

        <Field label="File" value={track.path} mono />
      </div>
    </div>
  );
}
