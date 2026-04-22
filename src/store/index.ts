import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

// Track type matching the Rust backend
export interface Track {
  id: number;
  path: string;
  dir_path: string;
  filename: string;
  title: string | null;
  artist: string | null;
  album: string | null;
  album_artist: string | null;
  year: number | null;
  genre: string | null;
  track_number: number | null;
  disc_number: number | null;
  duration_secs: number | null;
  cover_art_path: string | null;
  missing: boolean;
  bpm: number | null;
}

export type PlaybackState = "playing" | "paused" | "stopped";

export interface PlaybackPosition {
  position_secs: number;
  duration_secs: number;
}

export interface ScanProgress {
  files_found: number;
  current_dir: string;
  complete: boolean;
}

// Tauri event payloads
interface ScanProgressPayload {
  files_found: number;
  current_dir: string;
  complete: boolean;
}

interface PlaybackPositionPayload {
  position_secs: number;
  duration_secs: number;
}

interface PlaybackStateChangedPayload {
  state: PlaybackState;
  track_id: number | null;
}

interface WaveformReadyPayload {
  track_id: number;
}

interface SpectrogramReadyPayload {
  track_id: number;
}

interface QueueChangedPayload {
  queue: number[];
  current_index: number | null;
}

interface BpmReadyPayload {
  track_id: number;
  bpm: number | null;
}

interface GenreReadyPayload {
  track_id: number;
  genre: string;
}

interface KeybindActionPayload {
  action: string;
}

interface LibraryChangedPayload {}

// New types
export interface QueueState {
  trackIds: number[];
  currentIndex: number | null;
}

export type KeybindMap = Record<string, string>;

// Toast notification
export interface ToastMessage {
  id: string;
  message: string;
}

export interface Tag {
  id: number;
  name: string;
  color: string;
}

// Store state and actions
export interface MusicStore {
  // Library
  tracks: Track[];
  setTracks: (tracks: Track[]) => void;

  // Selection
  selectedTrackId: number | null;
  setSelectedTrackId: (id: number | null) => void;

  // Playback
  playbackState: PlaybackState;
  playbackPosition: PlaybackPosition;
  volume: number;
  setVolume: (volume: number) => void;

  // Search
  searchQuery: string;
  setSearchQuery: (query: string) => void;

  // Scan progress
  scanProgress: ScanProgress | null;

  // Visualization signals
  waveformReadyTrackId: number | null;
  spectrogramReadyTrackId: number | null;

  // Spectrogram display mode
  spectroMode: "static" | "live";
  setSpectroMode: (mode: "static" | "live") => void;

  // Tags
  tags: Tag[];
  setTags: (tags: Tag[]) => void;
  // track_id -> tag_id[]
  trackTagMap: Map<number, number[]>;
  setTrackTagMap: (pairs: [number, number][]) => void;
  // Active tag filter (null = no filter)
  activeTagIds: number[];
  setActiveTagIds: (ids: number[]) => void;

  // Tree refresh signal — increment to trigger FileExplorer to re-fetch the tree
  treeVersion: number;
  bumpTreeVersion: () => void;

  // Error state
  toasts: ToastMessage[];
  addToast: (message: string) => void;
  dismissToast: (id: string) => void;
  dbCorruptionVisible: boolean;
  showDbCorruption: () => void;
  hideDbCorruption: () => void;
  decodeErrorTrackIds: Set<number>;
  addDecodeError: (trackId: number) => void;
  clearDecodeError: (trackId: number) => void;

  // Queue
  queueTrackIds: number[];
  currentQueueIndex: number | null;
  setQueue: (ids: number[], index: number | null) => void;

  // EQ
  eqGains: number[];
  eqBypassed: boolean;
  setEqGains: (gains: number[]) => void;
  setEqBypassed: (bypassed: boolean) => void;

  // Pan
  panValue: number;
  setPanValue: (value: number) => void;

  // Crossfade
  crossfadeSecs: number;
  gaplessEnabled: boolean;
  setCrossfadeSecs: (secs: number) => void;
  setGaplessEnabled: (enabled: boolean) => void;

  // Keybinds
  keybindMap: KeybindMap;
  setKeybindMap: (map: KeybindMap) => void;

  // Discovery
  recommendations: Track[];
  setRecommendations: (tracks: Track[]) => void;

  // Ambient background
  ambientBgEnabled: boolean;
  ambientBgArtUrl: string | null;
  setAmbientBgEnabled: (enabled: boolean) => void;
  setAmbientBgArtUrl: (url: string | null) => void;

  // Discord presence
  discordPresenceEnabled: boolean;
  setDiscordPresenceEnabled: (enabled: boolean) => void;

  // Now Playing panel
  nowPlayingOpen: boolean;
  openNowPlaying: () => void;
  closeNowPlaying: () => void;

  // Command Palette
  commandPaletteOpen: boolean;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;

  // Active folder navigation
  activeFolder: string | null;
  setActiveFolder: (path: string | null) => void;
}

export const useMusicStore = create<MusicStore>((set) => ({
  tracks: [],
  setTracks: (tracks) => set({ tracks }),

  selectedTrackId: null,
  setSelectedTrackId: (id) => set({ selectedTrackId: id }),

  playbackState: "stopped",
  playbackPosition: { position_secs: 0, duration_secs: 0 },
  volume: 1.0,
  setVolume: (volume) => set({ volume }),

  searchQuery: "",
  setSearchQuery: (query) => set({ searchQuery: query }),

  scanProgress: null,

  waveformReadyTrackId: null,
  spectrogramReadyTrackId: null,

  spectroMode: "static",
  setSpectroMode: (mode) => set({ spectroMode: mode }),

  tags: [],
  setTags: (tags) => set({ tags }),
  trackTagMap: new Map(),
  setTrackTagMap: (pairs) => {
    const map = new Map<number, number[]>();
    for (const [trackId, tagId] of pairs) {
      const arr = map.get(trackId) ?? [];
      arr.push(tagId);
      map.set(trackId, arr);
    }
    set({ trackTagMap: map });
  },
  activeTagIds: [],
  setActiveTagIds: (ids) => set({ activeTagIds: ids }),

  treeVersion: 0,
  bumpTreeVersion: () => set((state) => ({ treeVersion: state.treeVersion + 1 })),

  // Error state
  toasts: [],
  addToast: (message) =>
    set((state) => ({
      toasts: [
        ...state.toasts,
        { id: `${Date.now()}-${Math.random()}`, message },
      ],
    })),
  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  dbCorruptionVisible: false,
  showDbCorruption: () => set({ dbCorruptionVisible: true }),
  hideDbCorruption: () => set({ dbCorruptionVisible: false }),
  decodeErrorTrackIds: new Set(),
  addDecodeError: (trackId) =>
    set((state) => ({
      decodeErrorTrackIds: new Set([...state.decodeErrorTrackIds, trackId]),
    })),
  clearDecodeError: (trackId) =>
    set((state) => {
      const next = new Set(state.decodeErrorTrackIds);
      next.delete(trackId);
      return { decodeErrorTrackIds: next };
    }),

  // Queue
  queueTrackIds: [],
  currentQueueIndex: null,
  setQueue: (ids, index) => set({ queueTrackIds: ids, currentQueueIndex: index }),

  // EQ
  eqGains: [0, 0, 0, 0, 0, 0, 0, 0],
  eqBypassed: false,
  setEqGains: (gains) => set({ eqGains: gains }),
  setEqBypassed: (bypassed) => set({ eqBypassed: bypassed }),

  // Pan
  panValue: 0,
  setPanValue: (value) => set({ panValue: value }),

  // Crossfade
  crossfadeSecs: 2,
  gaplessEnabled: true,
  setCrossfadeSecs: (secs) => set({ crossfadeSecs: secs }),
  setGaplessEnabled: (enabled) => set({ gaplessEnabled: enabled }),

  // Keybinds
  keybindMap: {},
  setKeybindMap: (map) => set({ keybindMap: map }),

  // Discovery
  recommendations: [],
  setRecommendations: (tracks) => set({ recommendations: tracks }),

  // Ambient background
  ambientBgEnabled: false,
  ambientBgArtUrl: null,
  setAmbientBgEnabled: (enabled) => set({ ambientBgEnabled: enabled }),
  setAmbientBgArtUrl: (url) => set({ ambientBgArtUrl: url }),

  // Discord presence
  discordPresenceEnabled: false,
  setDiscordPresenceEnabled: (enabled) => set({ discordPresenceEnabled: enabled }),

  // Now Playing panel
  nowPlayingOpen: false,
  openNowPlaying: () => set({ nowPlayingOpen: true }),
  closeNowPlaying: () => set({ nowPlayingOpen: false }),

  // Command Palette
  commandPaletteOpen: false,
  openCommandPalette: () => set({ commandPaletteOpen: true }),
  closeCommandPalette: () => set({ commandPaletteOpen: false }),

  // Active folder navigation
  activeFolder: null,
  setActiveFolder: (path) => set({ activeFolder: path }),
}));

// Register all Tauri event listeners and wire them to the store
export async function initEventListeners(): Promise<void> {
  await listen<ScanProgressPayload>("scan_progress", (event) => {
    useMusicStore.setState({ scanProgress: event.payload });
  });

  await listen<PlaybackPositionPayload>("playback_position", (event) => {
    useMusicStore.setState({ playbackPosition: event.payload });
  });

  await listen<PlaybackStateChangedPayload>("playback_state_changed", (event) => {
    const { state, track_id } = event.payload;
    useMusicStore.setState({
      playbackState: state,
      ...(track_id !== null && track_id !== undefined
        ? { selectedTrackId: track_id }
        : {}),
    });
  });

  await listen<WaveformReadyPayload>("waveform_ready", (event) => {
    useMusicStore.setState({ waveformReadyTrackId: event.payload.track_id });
  });

  await listen<SpectrogramReadyPayload>("spectrogram_ready", (event) => {
    useMusicStore.setState({ spectrogramReadyTrackId: event.payload.track_id });
  });

  await listen<LibraryChangedPayload>("library_changed", async () => {
    try {
      const tracks = await invoke<Track[]>("get_library");
      useMusicStore.setState((s) => ({ tracks, treeVersion: s.treeVersion + 1 }));
    } catch (_) {}
  });

  await listen<QueueChangedPayload>("queue_changed", (event) => {
    useMusicStore.setState({
      queueTrackIds: event.payload.queue,
      currentQueueIndex: event.payload.current_index,
    });
  });

  await listen<BpmReadyPayload>("bpm_ready", (event) => {
    const { track_id, bpm } = event.payload;
    useMusicStore.setState((state) => ({
      tracks: state.tracks.map((t) =>
        t.id === track_id ? { ...t, bpm } : t
      ),
    }));
  });

  await listen<GenreReadyPayload>("genre_ready", (event) => {
    const { track_id, genre } = event.payload;
    useMusicStore.setState((state) => ({
      tracks: state.tracks.map((t) =>
        t.id === track_id ? { ...t, genre } : t
      ),
    }));
  });

  await listen<KeybindActionPayload>("keybind_action", (event) => {
    const { action } = event.payload;
    const store = useMusicStore.getState();
    switch (action) {
      case "play_pause":
        invoke("pause").catch(() => {});
        break;
      case "next_track":
        invoke("play_next").catch(() => {});
        break;
      case "prev_track":
        invoke("play_previous").catch(() => {});
        break;
      case "volume_up": {
        const vol = Math.min(1.0, store.volume + 0.05);
        store.setVolume(vol);
        invoke("set_volume", { level: vol }).catch(() => {});
        break;
      }
      case "volume_down": {
        const vol = Math.max(0.0, store.volume - 0.05);
        store.setVolume(vol);
        invoke("set_volume", { level: vol }).catch(() => {});
        break;
      }
      case "seek_forward":
        invoke("seek", { positionSecs: store.playbackPosition.position_secs + 10 }).catch(() => {});
        break;
      case "seek_backward":
        invoke("seek", { positionSecs: Math.max(0, store.playbackPosition.position_secs - 10) }).catch(() => {});
        break;
      case "open_now_playing":
        store.openNowPlaying();
        break;
      case "close_now_playing":
        store.closeNowPlaying();
        break;
      case "command_palette":
        store.openCommandPalette();
        break;
    }
  });
}

export function initKeybindListener(): void {
  window.addEventListener("keydown", (event) => {
    const target = event.target as HTMLElement;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement
    ) {
      return;
    }

    const parts: string[] = [];
    if (event.ctrlKey) parts.push("Ctrl");
    if (event.altKey) parts.push("Alt");
    if (event.shiftKey) parts.push("Shift");
    if (event.metaKey) parts.push("Meta");
    parts.push(event.code);

    const combo = parts.join("+");
    invoke("dispatch_keybind", { combo }).catch(() => {});
  });
}
