import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
import { useMusicStore } from "./store/index";

interface AppState {
  last_track_id: number | null;
  last_position_secs: number;
  volume: number;
  root_directories: string[];
}

function App() {
  const { selectedTrackId, playbackPosition, volume, setVolume, setSelectedTrackId, setTags, setTrackTagMap } =
    useMusicStore();

  // Keep a ref to the loaded root_directories so we don't overwrite them
  const rootDirsRef = useRef<string[]>([]);

  useEffect(() => {
    invoke<AppState>("get_app_state")
      .then((state) => {
        rootDirsRef.current = state.root_directories;
        if (state.last_track_id != null) setSelectedTrackId(state.last_track_id);
        setVolume(state.volume);
        invoke("set_volume", { level: state.volume }).catch(() => {});
      })
      .catch(() => {});

    // Load tags and track-tag assignments
    invoke("get_tags").then((t: unknown) => setTags(t as import("./store/index").Tag[])).catch(() => {});
    invoke<[number, number][]>("get_all_track_tags").then((pairs) => setTrackTagMap(pairs)).catch(() => {});
  }, []);

  useEffect(() => {
    const state: AppState = {
      last_track_id: selectedTrackId,
      last_position_secs: playbackPosition.position_secs,
      volume,
      root_directories: rootDirsRef.current,
    };
    invoke("save_app_state", { state }).catch(() => {});
  }, [selectedTrackId, volume]);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    appWindow
      .onCloseRequested(async (event) => {
        event.preventDefault();
        const state: AppState = {
          last_track_id: selectedTrackId,
          last_position_secs: playbackPosition.position_secs,
          volume,
          root_directories: rootDirsRef.current,
        };
        try { await invoke("save_app_state", { state }); }
        finally { await appWindow.destroy(); }
      })
      .then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [selectedTrackId, playbackPosition.position_secs, volume]);

  return (
    <div className="app">
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
      <ToastContainer />
      <DbCorruptionModal />
    </div>
  );
}

export default App;
