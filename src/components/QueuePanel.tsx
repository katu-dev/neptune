import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useDroppable } from "@dnd-kit/core";
import { useMusicStore, type Track } from "../store/index";
import "./QueuePanel.css";

// ─── Icons ────────────────────────────────────────────────────────────────────

function IconRemove() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
    </svg>
  );
}

function IconPlayNext() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
    </svg>
  );
}

function IconDragHandle() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M11 18c0 1.1-.9 2-2 2s-2-.9-2-2 .9-2 2-2 2 .9 2 2zm-2-8c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0-6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm6 4c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z" />
    </svg>
  );
}

function IconShuffle() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z" />
    </svg>
  );
}

interface SortableQueueItemProps {
  track: Track;
  index: number;
  isCurrent: boolean;
  onRemove: (index: number) => void;
  onPlayNext: (trackId: number) => void;
}

function SortableQueueItem({
  track,
  index,
  isCurrent,
  onRemove,
  onPlayNext,
}: SortableQueueItemProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: `queue-${index}` });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  const title = track.title ?? track.filename;
  const subtitle = [track.artist, track.album].filter(Boolean).join(" — ");

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`queue-panel__item${isCurrent ? " queue-panel__item--current" : ""}${isDragging ? " queue-panel__item--dragging" : ""}`}
    >
      <button
        className="queue-panel__drag-handle"
        aria-label="Drag to reorder"
        {...attributes}
        {...listeners}
      >
        <IconDragHandle />
      </button>

      <div className="queue-panel__track-info">
        <span className="queue-panel__track-title">{title}</span>
        {subtitle && (
          <span className="queue-panel__track-subtitle">{subtitle}</span>
        )}
      </div>

      <div className="queue-panel__actions">
        <button
          className="queue-panel__action-btn"
          onClick={() => onPlayNext(track.id)}
          aria-label="Play next"
          title="Play next"
        >
          <IconPlayNext />
        </button>
        <button
          className="queue-panel__action-btn queue-panel__action-btn--remove"
          onClick={() => onRemove(index)}
          aria-label="Remove from queue"
          title="Remove"
        >
          <IconRemove />
        </button>
      </div>
    </div>
  );
}

// ─── Drop Zone for FileExplorer Tracks ────────────────────────────────────────

interface DropZoneProps {
  position: "top" | "bottom";
  isOver: boolean;
}

function DropZone({ position, isOver }: DropZoneProps) {
  return (
    <div
      className={`queue-panel__drop-zone queue-panel__drop-zone--${position}${isOver ? " queue-panel__drop-zone--active" : ""}`}
    >
      {isOver && (
        <div className="queue-panel__drop-indicator">
          Drop here to add to {position === "top" ? "front" : "end"} of queue
        </div>
      )}
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function QueuePanel() {
  const queueTrackIds = useMusicStore((s) => s.queueTrackIds);
  const currentQueueIndex = useMusicStore((s) => s.currentQueueIndex);
  const tracks = useMusicStore((s) => s.tracks);
  const addToast = useMusicStore((s) => s.addToast);

  const queueTracks = queueTrackIds
    .map((id) => tracks.find((t) => t.id === id))
    .filter((t): t is Track => t !== undefined);

  const { setNodeRef: setTopDropRef, isOver: isOverTop } = useDroppable({ id: "queue-drop-top" });
  const { setNodeRef: setBottomDropRef, isOver: isOverBottom } = useDroppable({ id: "queue-drop-bottom" });

  useEffect(() => {}, []);

  const handleRemove = async (index: number) => {
    try { await invoke("queue_remove", { index }); }
    catch (err) { console.error("Failed to remove from queue:", err); addToast("Failed to remove track from queue"); }
  };

  const handlePlayNext = async (trackId: number) => {
    try { await invoke("queue_add_next", { trackId }); }
    catch (err) { console.error("Failed to add track to queue:", err); addToast("Failed to add track to queue"); }
  };

  const handleShuffle = async () => {
    try { await invoke("queue_shuffle"); addToast("Queue shuffled"); }
    catch (err) { console.error("Failed to shuffle queue:", err); addToast("Failed to shuffle queue"); }
  };

  return (
    <div className="queue-panel">
      <div className="queue-panel__header">
        <div>
          <h2 className="queue-panel__title">Queue</h2>
          <span className="queue-panel__count">
            {queueTracks.length} {queueTracks.length === 1 ? "track" : "tracks"}
          </span>
        </div>
        <button className="queue-panel__shuffle-btn" onClick={handleShuffle} title="Shuffle queue">
          <IconShuffle />
        </button>
      </div>

      {/* Top drop zone for FileExplorer tracks */}
      <div ref={setTopDropRef}>
        <DropZone position="top" isOver={isOverTop} />
      </div>

      {/* Queue list */}
      <div className="queue-panel__list">
        {queueTracks.length === 0 ? (
          <div className="queue-panel__empty">
            Queue is empty. Add tracks from the library.
          </div>
        ) : (
          <SortableContext
            items={queueTracks.map((_, i) => `queue-${i}`)}
            strategy={verticalListSortingStrategy}
          >
            {queueTracks.map((track, index) => (
              <SortableQueueItem
                key={`queue-${index}`}
                track={track}
                index={index}
                isCurrent={currentQueueIndex === index}
                onRemove={handleRemove}
                onPlayNext={handlePlayNext}
              />
            ))}
          </SortableContext>
        )}
      </div>

      {/* Bottom drop zone for FileExplorer tracks */}
      <div ref={setBottomDropRef}>
        <DropZone position="bottom" isOver={isOverBottom} />
      </div>
    </div>
  );
}
