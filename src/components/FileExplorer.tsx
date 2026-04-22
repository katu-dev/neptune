import { useCallback, useEffect, useRef, useState } from "react";
import { FixedSizeList, type ListChildComponentProps } from "react-window";
import { invoke } from "@tauri-apps/api/core";
import { useDraggable } from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { useMusicStore, type Tag, type Track } from "../store/index";
import TagFilterBar from "./TagFilterBar";
import "./FileExplorer.css";

// Tauri serialises AppError as { type: "Io" | "Database" | "Decode" | ... , message: string }
interface TauriAppError {
  type: "Io" | "Database" | "Decode" | "TrackNotFound" | "UnsupportedFormat";
  message?: string;
}

// ─── Types ────────────────────────────────────────────────────────────────────

interface DirNode {
  path: string;
  name: string;
  children: DirNode[];
  tracks: Track[];
}

type FlatItem = { kind: "track"; track: Track; depth: number; key: string };

// ─── Tree flattening ──────────────────────────────────────────────────────────

function flattenAllTracks(nodes: DirNode[]): Track[] {
  const tracks: Track[] = [];
  for (const node of nodes) {
    tracks.push(...node.tracks);
    tracks.push(...flattenAllTracks(node.children));
  }
  return tracks;
}

// ─── Context Menu ─────────────────────────────────────────────────────────────

interface TrackContextMenuProps {
  x: number; y: number;
  track: Track;
  isFavorited: boolean;
  onClose: () => void;
  onPlay: () => void;
  onPlayNext: () => void;
  onAddToQueue: () => void;
  onToggleFavorite: () => void;
}

function TrackContextMenu({ x, y, isFavorited, onClose, onPlay, onPlayNext, onAddToQueue, onToggleFavorite }: TrackContextMenuProps) {
  // Close on outside click
  useEffect(() => {
    const handler = () => onClose();
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [onClose]);

  return (
    <div
      className="track-ctx-menu"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button className="track-ctx-menu__item" onClick={onPlay}>▶ Play</button>
      <button className="track-ctx-menu__item" onClick={onPlayNext}>⏭ Play Next</button>
      <button className="track-ctx-menu__item" onClick={onAddToQueue}>+ Add to Queue</button>
      <div className="track-ctx-menu__divider" />
      <button className="track-ctx-menu__item" onClick={onToggleFavorite}>
        {isFavorited ? "♥ Remove Favorite" : "♡ Add to Favorites"}
      </button>
    </div>
  );
}

// ─── Row renderer ─────────────────────────────────────────────────────────────

interface RowData {
  items: FlatItem[];
  selectedTrackId: number | null;
  focusedIndex: number;
  decodeErrorTrackIds: Set<number>;
  tags: Tag[];
  trackTagMap: Map<number, number[]>;
  favoriteTrackIds: Set<number>;
  onTrackSelect: (track: Track) => void;
  onRowFocus: (index: number) => void;
  onToggleFavorite: (trackId: number) => void;
  onAddToQueue: (trackId: number) => void;
  onPlayNext: (trackId: number) => void;
}

// ─── Draggable track row ──────────────────────────────────────────────────────

interface DraggableTrackRowProps {
  track: Track;
  style: React.CSSProperties;
  indent: number;
  isSelected: boolean;
  isFocused: boolean;
  hasDecodeError: boolean;
  trackTags: Tag[];
  isFavorited: boolean;
  onTrackSelect: (track: Track) => void;
  onRowFocus: (index: number) => void;
  onToggleFavorite: (trackId: number) => void;
  onAddToQueue: (trackId: number) => void;
  onPlayNext: (trackId: number) => void;
  rowIndex: number;
}

function DraggableTrackRow({
  track,
  style,
  indent,
  isSelected,
  isFocused,
  hasDecodeError,
  trackTags,
  isFavorited,
  onTrackSelect,
  onRowFocus,
  onToggleFavorite,
  onAddToQueue,
  onPlayNext,
  rowIndex,
}: DraggableTrackRowProps) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: `fe-${track.id}`,
    data: { trackId: track.id, source: "file-explorer" },
  });

  // Context menu state
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);

  const dragStyle: React.CSSProperties = {
    ...style,
    paddingLeft: indent,
    transform: CSS.Translate.toString(transform),
    opacity: isDragging ? 0.4 : 1,
    cursor: isDragging ? "grabbing" : "grab",
    zIndex: isDragging ? 1 : undefined,
  };

  const title = track.title ?? track.filename;
  const subtitle = [track.artist, track.album].filter(Boolean).join(" — ");

  // Extract role and tabIndex from dnd-kit attributes to avoid duplicate props
  const { role: _role, tabIndex: _tabIndex, ...restAttributes } = attributes;

  return (
    <>
      <div
        ref={setNodeRef}
        role="treeitem"
        aria-selected={isSelected}
        tabIndex={isFocused ? 0 : -1}
        className={[
          "file-explorer__row",
          "file-explorer__row--track",
          isSelected ? "file-explorer__row--selected" : "",
          track.missing ? "file-explorer__row--missing" : "",
          isFocused ? "file-explorer__row--focused" : "",
          isDragging ? "file-explorer__row--dragging" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        style={dragStyle}
        onClick={() => { onTrackSelect(track); onRowFocus(rowIndex); }}
        onFocus={() => onRowFocus(rowIndex)}
        onContextMenu={(e) => {
          e.preventDefault();
          setCtxMenu({ x: e.clientX, y: e.clientY });
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); onTrackSelect(track); }
        }}
        {...restAttributes}
        {...listeners}
      >
        {track.missing && (
          <span className="file-explorer__missing-icon" aria-label="Missing file" title="File not found on disk">⚠</span>
        )}
        <div className="file-explorer__track-info">
          <span className="file-explorer__track-title">{title}</span>
          {subtitle && <span className="file-explorer__track-subtitle">{subtitle}</span>}
        </div>
        {trackTags.length > 0 && (
          <div className="file-explorer__tag-dots" aria-label="Tags">
            {trackTags.map((tag) => (
              <span key={tag.id} className="file-explorer__tag-dot" style={{ background: tag.color }} title={tag.name} />
            ))}
          </div>
        )}
        <button
          className={`file-explorer__fav-btn${isFavorited ? " file-explorer__fav-btn--active" : ""}`}
          onClick={(e) => { e.stopPropagation(); onToggleFavorite(track.id); }}
          aria-label={isFavorited ? "Remove from favorites" : "Add to favorites"}
          title={isFavorited ? "Unfavorite" : "Favorite"}
        >
          {isFavorited ? "♥" : "♡"}
        </button>
        {hasDecodeError && (
          <span className="file-explorer__decode-error-badge" aria-label="Playback error" title="Could not decode this track">
            Decode error
          </span>
        )}
      </div>

      {/* Context menu */}
      {ctxMenu && (
        <TrackContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          track={track}
          isFavorited={isFavorited}
          onClose={() => setCtxMenu(null)}
          onPlay={() => { onTrackSelect(track); setCtxMenu(null); }}
          onPlayNext={() => { onPlayNext(track.id); setCtxMenu(null); }}
          onAddToQueue={() => { onAddToQueue(track.id); setCtxMenu(null); }}
          onToggleFavorite={() => { onToggleFavorite(track.id); setCtxMenu(null); }}
        />
      )}
    </>
  );
}

function Row({ index, style, data }: ListChildComponentProps<RowData>) {
  const { items, selectedTrackId, focusedIndex, decodeErrorTrackIds, tags, trackTagMap, favoriteTrackIds, onTrackSelect, onRowFocus, onToggleFavorite, onAddToQueue, onPlayNext } = data;
  const item = items[index];
  const isFocused = focusedIndex === index;
  const indent = 12;

  const { track } = item;
  const isSelected = selectedTrackId === track.id;
  const hasDecodeError = decodeErrorTrackIds.has(track.id);
  const trackTagIds = trackTagMap.get(track.id) ?? [];
  const trackTags = tags.filter((t) => trackTagIds.includes(t.id));
  const isFavorited = favoriteTrackIds.has(track.id);

  return (
    <DraggableTrackRow
      track={track}
      style={style}
      indent={indent}
      isSelected={isSelected}
      isFocused={isFocused}
      hasDecodeError={hasDecodeError}
      trackTags={trackTags}
      isFavorited={isFavorited}
      onTrackSelect={onTrackSelect}
      onRowFocus={onRowFocus}
      onToggleFavorite={onToggleFavorite}
      onAddToQueue={onAddToQueue}
      onPlayNext={onPlayNext}
      rowIndex={index}
    />
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

const ROW_HEIGHT = 48;

export default function FileExplorer() {
  const { selectedTrackId, setSelectedTrackId, searchQuery, setSearchQuery, addToast, showDbCorruption, addDecodeError, decodeErrorTrackIds, treeVersion, tags, trackTagMap, activeTagIds, favoriteTrackIds, toggleFavorite, showFavoritesOnly, setShowFavoritesOnly } =
    useMusicStore();

  const [tree, setTree] = useState<DirNode[]>([]);
  const [focusedIndex, setFocusedIndex] = useState(0);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const listRef = useRef<FixedSizeList<any>>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<DirNode[]>("get_directory_tree")
      .then((nodes) => setTree(nodes))
      .catch(() => setTree([]));
  }, [treeVersion]);

  // Always show a flat list of all tracks (no folder rows)
  const items: FlatItem[] = (() => {
    const q = searchQuery.trim().toLowerCase();
    const hasTagFilter = activeTagIds.length > 0;
    const allTracks = flattenAllTracks(tree);

    return allTracks
      .filter((t) => {
        if (showFavoritesOnly && !favoriteTrackIds.has(t.id)) return false;
        if (q) {
          const haystack = [t.title, t.artist, t.album].filter(Boolean).join(" ").toLowerCase();
          if (!haystack.includes(q)) return false;
        }
        if (hasTagFilter) {
          const tids = trackTagMap.get(t.id) ?? [];
          if (!activeTagIds.every((id) => tids.includes(id))) return false;
        }
        return true;
      })
      .map((t) => ({ kind: "track" as const, track: t, depth: 0, key: `track:${t.id}` }));
  })();

  // Library stats
  const allTracks = flattenAllTracks(tree);
  const totalDurationSecs = allTracks.reduce((sum, t) => sum + (t.duration_secs ?? 0), 0);
  const totalHours = Math.floor(totalDurationSecs / 3600);
  const totalMins = Math.floor((totalDurationSecs % 3600) / 60);
  const statsLabel = allTracks.length > 0
    ? `${allTracks.length} tracks · ${totalHours > 0 ? `${totalHours}h ` : ""}${totalMins}m`
    : "";

  const handleAddToQueue = useCallback(async (trackId: number) => {
    try { await invoke("queue_add", { trackId }); }
    catch (err) { addToast(`Failed to add to queue: ${err}`); }
  }, [addToast]);

  const handlePlayNext = useCallback(async (trackId: number) => {
    try { await invoke("queue_add_next", { trackId }); }
    catch (err) { addToast(`Failed to add to queue: ${err}`); }
  }, [addToast]);

  const handleTrackSelect = useCallback(
    async (track: Track) => {
      setSelectedTrackId(track.id);
      try {
        await invoke("play_track", { trackId: track.id });
      } catch (err) {
        const appErr = err as TauriAppError;
        if (appErr?.type === "Decode") {
          addDecodeError(track.id);
        } else if (appErr?.type === "Database") {
          showDbCorruption();
        } else if (appErr?.type === "Io") {
          addToast(`File error: ${appErr.message ?? "Could not read track file."}`);
        } else if (appErr?.type === "UnsupportedFormat") {
          addToast(`Unsupported format: ${appErr.message ?? "This audio format is not supported."}`);
        }
      }
    },
    [setSelectedTrackId, addDecodeError, showDbCorruption, addToast]
  );

  // Keyboard navigation on the container
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (items.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setFocusedIndex((prev) => {
          const next = Math.min(prev + 1, items.length - 1);
          listRef.current?.scrollToItem(next, "auto");
          return next;
        });
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setFocusedIndex((prev) => {
          const next = Math.max(prev - 1, 0);
          listRef.current?.scrollToItem(next, "auto");
          return next;
        });
      } else if (e.key === "Enter") {
        const item = items[focusedIndex];
        if (!item) return;
        handleTrackSelect(item.track);
      }
    },
    [items, focusedIndex, handleTrackSelect]
  );

  const rowData: RowData = {
    items,
    selectedTrackId,
    focusedIndex,
    decodeErrorTrackIds,
    tags,
    trackTagMap,
    favoriteTrackIds,
    onTrackSelect: handleTrackSelect,
    onRowFocus: setFocusedIndex,
    onToggleFavorite: toggleFavorite,
    onAddToQueue: handleAddToQueue,
    onPlayNext: handlePlayNext,
  };

  return (
    <div
      ref={containerRef}
      className="file-explorer"
      role="tree"
      aria-label="Music library"
      onKeyDown={handleKeyDown}
    >
      {/* Search + favorites filter */}
      <div className="file-explorer__search">
        <input
          type="search"
          className="file-explorer__search-input"
          placeholder="Filter by title, artist, or album…"
          aria-label="Search tracks"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        <button
          className={`file-explorer__fav-filter${showFavoritesOnly ? " file-explorer__fav-filter--active" : ""}`}
          onClick={() => setShowFavoritesOnly(!showFavoritesOnly)}
          aria-label={showFavoritesOnly ? "Show all tracks" : "Show favorites only"}
          title={showFavoritesOnly ? "Show all" : "Favorites only"}
        >
          {showFavoritesOnly ? "♥" : "♡"}
        </button>
      </div>

      {/* Library stats */}
      {statsLabel && (
        <div className="file-explorer__stats">{statsLabel}</div>
      )}

      {/* Tag filter bar */}
      <TagFilterBar />

      {/* Virtualized list */}
      <div className="file-explorer__list">
        {items.length === 0 ? (
          <div className="file-explorer__empty">
            {searchQuery || activeTagIds.length > 0 || showFavoritesOnly
              ? "No tracks match your filter."
              : "No library loaded. Scan a folder to get started."}
          </div>
        ) : (
          <AutoSizedList listRef={listRef} itemCount={items.length} itemData={rowData} />
        )}
      </div>
    </div>
  );
}

// ─── Auto-sized wrapper ───────────────────────────────────────────────────────

interface AutoSizedListProps {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  listRef: React.RefObject<FixedSizeList<any> | null>;
  itemCount: number;
  itemData: RowData;
}

function AutoSizedList({ listRef, itemCount, itemData }: AutoSizedListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(400);

  useEffect(() => {
    if (!containerRef.current) return;
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) setHeight(entry.contentRect.height);
    });
    ro.observe(containerRef.current);
    return () => ro.disconnect();
  }, []);

  return (
    <div ref={containerRef} style={{ height: "100%" }}>
      <FixedSizeList
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        ref={listRef as React.RefObject<FixedSizeList<RowData>>}
        height={height}
        width="100%"
        itemCount={itemCount}
        itemSize={ROW_HEIGHT}
        itemData={itemData}
        itemKey={(index, data) => data.items[index].key}
      >
        {Row}
      </FixedSizeList>
    </div>
  );
}
