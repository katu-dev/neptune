import { useCallback, useEffect, useRef, useState } from "react";
import { FixedSizeList, type ListChildComponentProps } from "react-window";
import { invoke } from "@tauri-apps/api/core";
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

type FlatItem =
  | { kind: "folder"; node: DirNode; depth: number; expanded: boolean; key: string }
  | { kind: "track"; track: Track; depth: number; key: string };

// ─── Tree flattening ──────────────────────────────────────────────────────────

function flattenTree(
  nodes: DirNode[],
  expandedPaths: Set<string>,
  depth = 0
): FlatItem[] {
  const items: FlatItem[] = [];
  for (const node of nodes) {
    const expanded = expandedPaths.has(node.path);
    items.push({ kind: "folder", node, depth, expanded, key: `folder:${node.path}` });
    if (expanded) {
      items.push(...flattenTree(node.children, expandedPaths, depth + 1));
      for (const track of node.tracks) {
        items.push({ kind: "track", track, depth: depth + 1, key: `track:${track.id}` });
      }
    }
  }
  return items;
}

function flattenAllTracks(nodes: DirNode[]): Track[] {
  const tracks: Track[] = [];
  for (const node of nodes) {
    tracks.push(...node.tracks);
    tracks.push(...flattenAllTracks(node.children));
  }
  return tracks;
}

// ─── Row renderer ─────────────────────────────────────────────────────────────

interface RowData {
  items: FlatItem[];
  selectedTrackId: number | null;
  focusedIndex: number;
  decodeErrorTrackIds: Set<number>;
  tags: Tag[];
  trackTagMap: Map<number, number[]>;
  onFolderToggle: (path: string) => void;
  onTrackSelect: (track: Track) => void;
  onRowFocus: (index: number) => void;
}

function Row({ index, style, data }: ListChildComponentProps<RowData>) {
  const { items, selectedTrackId, focusedIndex, decodeErrorTrackIds, tags, trackTagMap, onFolderToggle, onTrackSelect, onRowFocus } = data;
  const item = items[index];
  const isFocused = focusedIndex === index;

  const indent = item.depth * 16 + 12; // 16px per level + 12px base

  if (item.kind === "folder") {
    return (
      <div
        role="treeitem"
        aria-expanded={item.expanded}
        tabIndex={isFocused ? 0 : -1}
        className={`file-explorer__row file-explorer__row--folder${isFocused ? " file-explorer__row--focused" : ""}`}
        style={{ ...style, paddingLeft: indent }}
        onClick={() => { onFolderToggle(item.node.path); onRowFocus(index); }}
        onFocus={() => onRowFocus(index)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onFolderToggle(item.node.path);
          }
        }}
      >
        <span className="file-explorer__toggle" aria-hidden="true">
          {item.expanded ? "▾" : "▸"}
        </span>
        <span className="file-explorer__folder-icon" aria-hidden="true">📁</span>
        <span className="file-explorer__track-title">{item.node.name}</span>
      </div>
    );
  }

  // Track row
  const { track } = item;
  const isSelected = selectedTrackId === track.id;
  const hasDecodeError = decodeErrorTrackIds.has(track.id);
  const title = track.title ?? track.filename;
  const subtitle = [track.artist, track.album].filter(Boolean).join(" — ");
  const trackTagIds = trackTagMap.get(track.id) ?? [];
  const trackTags = tags.filter((t) => trackTagIds.includes(t.id));

  return (
    <div
      role="treeitem"
      aria-selected={isSelected}
      tabIndex={isFocused ? 0 : -1}
      className={[
        "file-explorer__row",
        "file-explorer__row--track",
        isSelected ? "file-explorer__row--selected" : "",
        track.missing ? "file-explorer__row--missing" : "",
        isFocused ? "file-explorer__row--focused" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{ ...style, paddingLeft: indent }}
      onClick={() => { onTrackSelect(track); onRowFocus(index); }}
      onFocus={() => onRowFocus(index)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onTrackSelect(track);
        }
      }}
    >
      {track.missing && (
        <span className="file-explorer__missing-icon" aria-label="Missing file" title="File not found on disk">
          ⚠
        </span>
      )}
      <div className="file-explorer__track-info">
        <span className="file-explorer__track-title">{title}</span>
        {subtitle && (
          <span className="file-explorer__track-subtitle">{subtitle}</span>
        )}
      </div>
      {trackTags.length > 0 && (
        <div className="file-explorer__tag-dots" aria-label="Tags">
          {trackTags.map((tag) => (
            <span
              key={tag.id}
              className="file-explorer__tag-dot"
              style={{ background: tag.color }}
              title={tag.name}
            />
          ))}
        </div>
      )}
      {hasDecodeError && (
        <span
          className="file-explorer__decode-error-badge"
          aria-label="Playback error"
          title="Could not decode this track"
        >
          Decode error
        </span>
      )}
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

const ROW_HEIGHT = 48;

export default function FileExplorer() {
  const { selectedTrackId, setSelectedTrackId, searchQuery, setSearchQuery, addToast, showDbCorruption, addDecodeError, decodeErrorTrackIds, treeVersion, tags, trackTagMap, activeTagIds } =
    useMusicStore();

  const [tree, setTree] = useState<DirNode[]>([]);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [focusedIndex, setFocusedIndex] = useState(0);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const listRef = useRef<FixedSizeList<any>>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Load directory tree on mount and whenever a scan completes
  useEffect(() => {
    invoke<DirNode[]>("get_directory_tree")
      .then((nodes) => {
        setTree(nodes);
        // Auto-expand top-level folders
        const topPaths = new Set(nodes.map((n) => n.path));
        setExpandedPaths(topPaths);
      })
      .catch(() => {
        // Tree unavailable (no library scanned yet) — show empty state
        setTree([]);
      });
  }, [treeVersion]);

  // Build the flat list of visible items
  const items: FlatItem[] = (() => {
    const q = searchQuery.trim().toLowerCase();
    const hasTagFilter = activeTagIds.length > 0;

    if (!q && !hasTagFilter) {
      return flattenTree(tree, expandedPaths);
    }

    // Flat mode: search or tag filter active
    const allTracks = flattenAllTracks(tree);
    return allTracks
      .filter((t) => {
        if (q) {
          const haystack = [t.title, t.artist, t.album]
            .filter(Boolean)
            .join(" ")
            .toLowerCase();
          if (!haystack.includes(q)) return false;
        }
        if (hasTagFilter) {
          const tids = trackTagMap.get(t.id) ?? [];
          // Track must have ALL active tag ids
          if (!activeTagIds.every((id) => tids.includes(id))) return false;
        }
        return true;
      })
      .map((t) => ({ kind: "track" as const, track: t, depth: 0, key: `track:${t.id}` }));
  })();

  const handleFolderToggle = useCallback((path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

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
        if (item.kind === "folder") {
          handleFolderToggle(item.node.path);
        } else {
          handleTrackSelect(item.track);
        }
      }
    },
    [items, focusedIndex, handleFolderToggle, handleTrackSelect]
  );

  const rowData: RowData = {
    items,
    selectedTrackId,
    focusedIndex,
    decodeErrorTrackIds,
    tags,
    trackTagMap,
    onFolderToggle: handleFolderToggle,
    onTrackSelect: handleTrackSelect,
    onRowFocus: setFocusedIndex,
  };

  return (
    <div
      ref={containerRef}
      className="file-explorer"
      role="tree"
      aria-label="Music library"
      onKeyDown={handleKeyDown}
    >
      {/* Search input (task 15.2) */}
      <div className="file-explorer__search">
        <input
          type="search"
          className="file-explorer__search-input"
          placeholder="Filter by title, artist, or album…"
          aria-label="Search tracks"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
      </div>

      {/* Tag filter bar */}
      <TagFilterBar />

      {/* Virtualized tree list */}
      <div className="file-explorer__list">
        {items.length === 0 ? (
          <div className="file-explorer__empty">
            {searchQuery || activeTagIds.length > 0 ? "No tracks match your search." : "No library loaded. Scan a folder to get started."}
          </div>
        ) : (
          <AutoSizedList
            listRef={listRef}
            itemCount={items.length}
            itemData={rowData}
          />
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
