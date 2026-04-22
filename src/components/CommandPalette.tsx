import { useEffect, useRef, useState, useCallback } from "react";
import Fuse from "fuse.js";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import "./CommandPalette.css";

export type CommandPaletteItemKind = "track" | "folder" | "action";

export interface CommandPaletteItem {
  kind: CommandPaletteItemKind;
  id: string;
  label: string;
  sublabel?: string;
  score?: number;
}

// Registered action names derived from the default keybind map
const REGISTERED_ACTIONS = [
  "play_pause",
  "next_track",
  "prev_track",
  "volume_up",
  "volume_down",
  "seek_forward",
  "seek_backward",
  "command_palette",
  "open_now_playing",
  "close_now_playing",
];

export default function CommandPalette() {
  const { commandPaletteOpen, closeCommandPalette, tracks, keybindMap, setActiveFolder } =
    useMusicStore();

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<CommandPaletteItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const fuseRef = useRef<Fuse<CommandPaletteItem> | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  // Build Fuse index whenever tracks change
  useEffect(() => {
    const items: CommandPaletteItem[] = [];

    // Tracks
    for (const track of tracks) {
      items.push({
        kind: "track",
        id: String(track.id),
        label: track.title ?? track.filename,
        sublabel: [track.artist, track.album].filter(Boolean).join(" — ") || undefined,
      });
    }

    // Folder paths (unique dir_paths from tracks)
    const folderSet = new Set<string>();
    for (const track of tracks) {
      if (track.dir_path) folderSet.add(track.dir_path);
    }
    for (const path of folderSet) {
      items.push({ kind: "folder", id: path, label: path });
    }

    // Registered actions (from keybindMap keys + defaults)
    const actionNames = new Set([
      ...REGISTERED_ACTIONS,
      ...Object.keys(keybindMap),
    ]);
    for (const name of actionNames) {
      items.push({ kind: "action", id: name, label: name });
    }

    fuseRef.current = new Fuse(items, {
      keys: ["label", "sublabel"],
      threshold: 0.4,
      includeScore: true,
    });
  }, [tracks, keybindMap]);

  // Run search on query change
  useEffect(() => {
    if (!fuseRef.current) return;
    if (query.trim() === "") {
      setResults([]);
      setSelectedIndex(0);
      return;
    }
    const raw = fuseRef.current.search(query).slice(0, 20);
    const mapped: CommandPaletteItem[] = raw.map((r) => ({
      ...r.item,
      score: r.score,
    }));
    setResults(mapped);
    setSelectedIndex(0);
  }, [query]);

  // Focus input when opened; reset state when closed
  useEffect(() => {
    if (commandPaletteOpen) {
      setQuery("");
      setResults([]);
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [commandPaletteOpen]);

  // Scroll selected item into view
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const item = list.children[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const handleSelect = useCallback(
    async (item: CommandPaletteItem) => {
      closeCommandPalette();
      if (item.kind === "track") {
        try {
          await invoke("play_track", { trackId: parseInt(item.id, 10) });
        } catch (err) {
          console.error("CommandPalette: play_track failed", err);
        }
      } else if (item.kind === "folder") {
        setActiveFolder(item.id);
      } else if (item.kind === "action") {
        // Re-use the keybind_action event path by dispatching directly to the store
        const store = useMusicStore.getState();
        switch (item.id) {
          case "play_pause": invoke("pause").catch(() => {}); break;
          case "next_track": invoke("play_next").catch(() => {}); break;
          case "prev_track": invoke("play_previous").catch(() => {}); break;
          case "volume_up": { const v = Math.min(1, store.volume + 0.05); store.setVolume(v); invoke("set_volume", { level: v }).catch(() => {}); break; }
          case "volume_down": { const v = Math.max(0, store.volume - 0.05); store.setVolume(v); invoke("set_volume", { level: v }).catch(() => {}); break; }
          case "seek_forward": invoke("seek", { positionSecs: store.playbackPosition.position_secs + 10 }).catch(() => {}); break;
          case "seek_backward": invoke("seek", { positionSecs: Math.max(0, store.playbackPosition.position_secs - 10) }).catch(() => {}); break;
          case "open_now_playing": store.openNowPlaying(); break;
          case "close_now_playing": store.closeNowPlaying(); break;
          case "command_palette": store.openCommandPalette(); break;
        }
      }
    },
    [closeCommandPalette, setActiveFolder]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const item = results[selectedIndex];
        if (item) handleSelect(item);
      } else if (e.key === "Escape") {
        e.preventDefault();
        closeCommandPalette();
      }
    },
    [results, selectedIndex, handleSelect, closeCommandPalette]
  );

  if (!commandPaletteOpen) return null;

  return (
    <div
      className="command-palette-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Command Palette"
      onMouseDown={(e) => {
        // Close when clicking the backdrop (not the panel itself)
        if (e.target === e.currentTarget) closeCommandPalette();
      }}
    >
      <div className="command-palette">
        <input
          ref={inputRef}
          className="command-palette__input"
          type="text"
          placeholder="Search tracks, folders, actions…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          aria-label="Command palette search"
          aria-autocomplete="list"
          aria-controls="command-palette-list"
          aria-activedescendant={
            results.length > 0 ? `cp-item-${selectedIndex}` : undefined
          }
          autoComplete="off"
          spellCheck={false}
        />
        {results.length > 0 && (
          <ul
            ref={listRef}
            id="command-palette-list"
            className="command-palette__list"
            role="listbox"
          >
            {results.map((item, i) => (
              <li
                key={`${item.kind}-${item.id}`}
                id={`cp-item-${i}`}
                className={`command-palette__item${i === selectedIndex ? " command-palette__item--selected" : ""}`}
                role="option"
                aria-selected={i === selectedIndex}
                onMouseEnter={() => setSelectedIndex(i)}
                onMouseDown={(e) => {
                  e.preventDefault();
                  handleSelect(item);
                }}
              >
                <span className={`command-palette__kind command-palette__kind--${item.kind}`}>
                  {item.kind === "track" ? "♪" : item.kind === "folder" ? "📁" : "⚡"}
                </span>
                <span className="command-palette__label">{item.label}</span>
                {item.sublabel && (
                  <span className="command-palette__sublabel">{item.sublabel}</span>
                )}
              </li>
            ))}
          </ul>
        )}
        {query.trim() !== "" && results.length === 0 && (
          <div className="command-palette__empty">No results for "{query}"</div>
        )}
      </div>
    </div>
  );
}
