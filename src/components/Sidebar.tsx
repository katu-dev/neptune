import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store/index";
import SettingsPanel from "./SettingsPanel";
import FoldersPanel from "./FoldersPanel";
import TagsPanel from "./TagsPanel";
import QueuePanel from "./QueuePanel";
import DiscoveryFeed from "./DiscoveryFeed";
import logoUrl from "../assets/logo.svg";
import "./Sidebar.css";

const NAV_ITEMS = ["Library", "Queue", "Discover", "Now Playing", "Folders", "Tags", "Settings"] as const;

export default function Sidebar() {
  const { addToast, bumpTreeVersion, openNowPlaying } = useMusicStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [foldersOpen, setFoldersOpen] = useState(false);
  const [tagsOpen, setTagsOpen] = useState(false);
  const [queueOpen, setQueueOpen] = useState(false);
  const [discoverOpen, setDiscoverOpen] = useState(false);

  async function handleAddFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Music Folder",
      });
      if (selected == null) return;
      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;
      await invoke("scan_directory", { path });
      bumpTreeVersion();
    } catch (err) {
      addToast(`Failed to add folder: ${err}`);
    }
  }

  function handleNavClick(item: typeof NAV_ITEMS[number]) {
    if (item === "Folders") setFoldersOpen(true);
    else if (item === "Tags") setTagsOpen(true);
    else if (item === "Settings") setSettingsOpen(true);
    else if (item === "Queue") setQueueOpen(true);
    else if (item === "Discover") setDiscoverOpen(true);
    else if (item === "Now Playing") openNowPlaying();
  }

  return (
    <>
      <nav className="sidebar">
        <div className="sidebar__header">
          <img src={logoUrl} alt="Music Explorer logo" width={32} height={32} />
          <span className="sidebar__app-name">Neptune</span>
        </div>
        <ul className="sidebar__nav">
          {NAV_ITEMS.map((item) => (
            <li
              key={item}
              className="sidebar__nav-item"
              onClick={() => handleNavClick(item)}
            >
              {item}
            </li>
          ))}
        </ul>
        <div className="sidebar__actions">
          <button className="sidebar__add-folder-btn" onClick={handleAddFolder}>
            + Add Folder
          </button>
        </div>
      </nav>
      {foldersOpen && <FoldersPanel onClose={() => setFoldersOpen(false)} />}
      {tagsOpen && <TagsPanel onClose={() => setTagsOpen(false)} />}
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      {queueOpen && (
        <div className="sidebar-panel-overlay" role="dialog" aria-modal="true" aria-label="Queue">
          <div className="sidebar-panel-overlay__backdrop" onClick={() => setQueueOpen(false)} />
          <div className="sidebar-panel-overlay__panel">
            <div className="sidebar-panel-overlay__header">
              <span className="sidebar-panel-overlay__title">Queue</span>
              <button className="sidebar-panel-overlay__close" onClick={() => setQueueOpen(false)} aria-label="Close queue">✕</button>
            </div>
            <div className="sidebar-panel-overlay__body">
              <QueuePanel />
            </div>
          </div>
        </div>
      )}
      {discoverOpen && (
        <div className="sidebar-panel-overlay" role="dialog" aria-modal="true" aria-label="Discover">
          <div className="sidebar-panel-overlay__backdrop" onClick={() => setDiscoverOpen(false)} />
          <div className="sidebar-panel-overlay__panel">
            <div className="sidebar-panel-overlay__header">
              <span className="sidebar-panel-overlay__title">Discover</span>
              <button className="sidebar-panel-overlay__close" onClick={() => setDiscoverOpen(false)} aria-label="Close discover">✕</button>
            </div>
            <div className="sidebar-panel-overlay__body">
              <DiscoveryFeed />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
