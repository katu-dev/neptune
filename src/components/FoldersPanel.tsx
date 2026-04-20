import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useMusicStore } from "../store/index";
import "./FoldersPanel.css";

interface Props {
  onClose: () => void;
}

interface AppState {
  root_directories: string[];
}

export default function FoldersPanel({ onClose }: Props) {
  const { addToast, bumpTreeVersion } = useMusicStore();
  const [folders, setFolders] = useState<string[]>([]);
  const [scanning, setScanning] = useState<string | null>(null);

  async function load() {
    try {
      const state = await invoke<AppState>("get_app_state");
      setFolders(state.root_directories);
    } catch {
      setFolders([]);
    }
  }

  useEffect(() => { load(); }, []);

  async function handleAdd() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Select Music Folder" });
      if (!selected) return;
      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;
      setScanning(path);
      await invoke("scan_directory", { path });
      bumpTreeVersion();
      await load();
    } catch (err) {
      addToast(`Failed to add folder: ${err}`);
    } finally {
      setScanning(null);
    }
  }

  async function handleRescan(path: string) {
    try {
      setScanning(path);
      await invoke("scan_directory", { path });
      bumpTreeVersion();
      addToast(`Rescanned: ${path}`);
    } catch (err) {
      addToast(`Rescan failed: ${err}`);
    } finally {
      setScanning(null);
    }
  }

  async function handleRemove(path: string) {
    try {
      await invoke("remove_root_directory", { path });
      await load();
      bumpTreeVersion();
    } catch (err) {
      addToast(`Failed to remove folder: ${err}`);
    }
  }

  return (
    <div className="folders-overlay" role="dialog" aria-modal="true" aria-label="Folders">
      <div className="folders-overlay__backdrop" onClick={onClose} />
      <div className="folders-panel">
        <div className="folders-panel__header">
          <span className="folders-panel__title">Music Folders</span>
          <button className="folders-panel__close" onClick={onClose} aria-label="Close">✕</button>
        </div>
        <div className="folders-panel__body">
          {folders.length === 0 ? (
            <div className="folders-panel__empty">No folders added yet.</div>
          ) : (
            <ul className="folders-panel__list">
              {folders.map((f) => (
                <li key={f} className="folders-panel__item">
                  <span className="folders-panel__path" title={f}>{f}</span>
                  <div className="folders-panel__actions">
                    <button
                      className="folders-panel__btn"
                      onClick={() => handleRescan(f)}
                      disabled={scanning !== null}
                      title="Rescan"
                    >
                      {scanning === f ? "…" : "↺"}
                    </button>
                    <button
                      className="folders-panel__btn folders-panel__btn--danger"
                      onClick={() => handleRemove(f)}
                      disabled={scanning !== null}
                      title="Remove"
                    >
                      ✕
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          <button className="folders-panel__add-btn" onClick={handleAdd} disabled={scanning !== null}>
            {scanning && scanning !== folders.find(f => f === scanning) ? "Scanning…" : "+ Add Folder"}
          </button>
        </div>
      </div>
    </div>
  );
}
