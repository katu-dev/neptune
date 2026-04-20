import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

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
}
