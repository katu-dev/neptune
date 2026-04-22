import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  DndContext,
  DragOverlay,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import "./styles/tokens.css";
import Layout from "./components/Layout";
import Sidebar from "./components/Sidebar";
import TitleBar from "./components/TitleBar";
import FileExplorer from "./components/FileExplorer";
import MetadataPanel from "./components/MetadataPanel";
import PlaybackControls from "./components/PlaybackControls";
import WaveformView from "./components/WaveformView";
import SpectrumVisualizer from "./components/SpectrumVisualizer";
import VuMeter from "./components/VuMeter";
import ToastContainer from "./components/Toast";
import DbCorruptionModal from "./components/DbCorruptionModal";
import AmbientBackground from "./components/AmbientBackground";
import NowPlayingView from "./components/NowPlayingView";
import CommandPalette from "./components/CommandPalette";
import { useMusicStore, type Track } from "./store/index";

interface AppState {
  last_track_id: number | null;
  last_position_secs: number;
  volume: number;
  root_directories: string[];
}

function App() {
  const { selectedTrackId, playbackPosition, volume, setVolume, setSelectedTrackId, setTags, setTrackTagMap, setTracks, tracks, queueTrackIds } =
    useMusicStore();

  // Keep a ref to the loaded root_directories so we don't overwrite them
  const rootDirsRef = useRef<string[]>([]);

  // Refs for values needed in close handler — avoids re-registering the listener
  const selectedTrackIdRef = useRef(selectedTrackId);
  const playbackPositionRef = useRef(playbackPosition);
  const volumeRef = useRef(volume);
  useEffect(() => { selectedTrackIdRef.current = selectedTrackId; }, [selectedTrackId]);
  useEffect(() => { playbackPositionRef.current = playbackPosition; }, [playbackPosition]);
  useEffect(() => { volumeRef.current = volume; }, [volume]);

  // Drag-and-drop state
  const [activeId, setActiveId] = useState<string | number | null>(null);

  // Drag sensors
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8, // 8px movement required to start drag
      },
    })
  );

  useEffect(() => {
    invoke<AppState>("get_app_state")
      .then((state) => {
        rootDirsRef.current = state.root_directories;
        if (state.last_track_id != null) setSelectedTrackId(state.last_track_id);
        setVolume(state.volume);
        invoke("set_volume", { level: state.volume }).catch(() => {});
      })
      .catch(() => {});

    // Populate the tracks store so NowPlayingView, DiscoveryFeed, etc. can look up track metadata
    invoke<Track[]>("get_library")
      .then((library) => setTracks(library))
      .catch(() => {});

    // Load tags and track-tag assignments
    invoke("get_tags").then((t: unknown) => setTags(t as import("./store/index").Tag[])).catch(() => {});
    invoke<[number, number][]>("get_all_track_tags").then((pairs) => setTrackTagMap(pairs)).catch(() => {});
  }, []);

  // Debounced save_app_state — only fires 500ms after last change
  useEffect(() => {
    const timer = setTimeout(() => {
      const state: AppState = {
        last_track_id: selectedTrackId,
        last_position_secs: playbackPosition.position_secs,
        volume,
        root_directories: rootDirsRef.current,
      };
      invoke("save_app_state", { state }).catch(() => {});
    }, 500);
    return () => clearTimeout(timer);
  }, [selectedTrackId, volume]);

  // Register close handler once — uses refs to avoid stale closures
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    appWindow
      .onCloseRequested(async (event) => {
        event.preventDefault();
        const state: AppState = {
          last_track_id: selectedTrackIdRef.current,
          last_position_secs: playbackPositionRef.current.position_secs,
          volume: volumeRef.current,
          root_directories: rootDirsRef.current,
        };
        try { await invoke("save_app_state", { state }); }
        finally { await appWindow.destroy(); }
      })
      .then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []); // Empty deps — register once, use refs for current values

  // Determine the active dragged track for the overlay
  const activeDragTrack: Track | null = (() => {
    if (activeId === null) return null;
    // FileExplorer drags use "fe-<id>" string IDs
    if (typeof activeId === "string" && activeId.startsWith("fe-")) {
      const id = parseInt(activeId.replace("fe-", ""), 10);
      return tracks.find((t) => t.id === id) ?? null;
    }
    // Queue drags use "queue-<index>" string IDs
    if (typeof activeId === "string" && activeId.startsWith("queue-")) {
      const index = parseInt(activeId.replace("queue-", ""), 10);
      const trackId = queueTrackIds[index];
      return tracks.find((t) => t.id === trackId) ?? null;
    }
    return null;
  })();

  const handleDragStart = (event: DragStartEvent) => {
    setActiveId(event.active.id as string | number);
  };

  const handleDragEnd = async (event: DragEndEvent) => {
    const { active, over } = event;
    setActiveId(null);

    if (!over) return;

    // Handle FileExplorer track dropped onto queue drop zones
    const activeData = active.data.current as { trackId?: number; source?: string } | undefined;
    if (activeData?.source === "file-explorer" && activeData.trackId !== undefined) {
      const trackId = activeData.trackId;
      try {
        if (over.id === "queue-drop-top") {
          await invoke("queue_add_next", { trackId });
        } else if (over.id === "queue-drop-bottom") {
          await invoke("queue_add", { trackId });
        } else if (over.id === "now-playing-drop") {
          await invoke("queue_add_next", { trackId });
          await invoke("play_track", { trackId });
        }
      } catch (err) {
        console.error("Failed to add track to queue via drag:", err);
      }
      return;
    }

    // Handle queue reordering (queue-N to queue-M)
    const activeId = active.id as string;
    const overId = over.id as string;
    if (activeId.startsWith("queue-") && overId.startsWith("queue-")) {
      if (activeId === overId) return;
      const activeIndex = parseInt(activeId.replace("queue-", ""), 10);
      const overIndex = parseInt(overId.replace("queue-", ""), 10);
      if (isNaN(activeIndex) || isNaN(overIndex)) return;
      try {
        await invoke("queue_move", { from: activeIndex, to: overIndex });
      } catch (err) {
        console.error("Failed to reorder queue:", err);
      }
    }
  };

  const handleDragCancel = () => {
    setActiveId(null);
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      <div className="app">
        <AmbientBackground />
        <Layout
          titleBar={<TitleBar />}
          sidebar={<Sidebar />}
          fileExplorer={<FileExplorer />}
          metadataPanel={<MetadataPanel />}
          waveformBar={<WaveformView />}
          spectrum={<SpectrumVisualizer />}
          playbackControls={<PlaybackControls />}
          vuMeter={<VuMeter />}
        />
        <NowPlayingView />
        <CommandPalette />
        <ToastContainer />
        <DbCorruptionModal />

        {/* Shared DragOverlay for FileExplorer and Queue drags */}
        <DragOverlay>
          {activeDragTrack ? (
            <div className="file-explorer__drag-ghost">
              <div className="file-explorer__track-info">
                <span className="file-explorer__track-title">
                  {activeDragTrack.title ?? activeDragTrack.filename}
                </span>
                {(activeDragTrack.artist || activeDragTrack.album) && (
                  <span className="file-explorer__track-subtitle">
                    {[activeDragTrack.artist, activeDragTrack.album].filter(Boolean).join(" — ")}
                  </span>
                )}
              </div>
            </div>
          ) : null}
        </DragOverlay>
      </div>
    </DndContext>
  );
}

export default App;
