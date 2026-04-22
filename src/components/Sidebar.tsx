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
type NavItem = typeof NAV_ITEMS[number];

const NAV_ICONS: Record<NavItem, string> = {
  Library: "♪",
  Queue: "≡",
  Discover: "✦",
  "Now Playing": "▶",
  Folders: "📁",
  Tags: "🏷",
  Settings: "⚙",
};

export default function Sidebar() {
  const { addToast, bumpTreeVersion, openNowPlaying } = useMusicStore();
  const [activePanel, setActivePanel] = useState<NavItem | null>(null);

  async function handleAddFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Select Music Folder" });
      if (selected == null) return;
      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;
      await invoke("scan_directory", { path });
      bumpTreeVersion();
    } catch (err) {
      addToast(`Failed to add folder: ${err}`);
    }
  }

  function handleNavClick(item: NavItem) {
    if (item === "Now Playing") { openNowPlaying(); return; }
    setActivePanel((prev) => (prev === item ? null : item));
  }

  function closePanel() { setActivePanel(null); }

  return (
    <>
      <nav className="sidebar">
        <div className="sidebar__header">
          <img src={logoUrl} alt="Neptune logo" width={32} height={32} />
          <span className="sidebar__app-name">Neptune</span>
        </div>
        <ul className="sidebar__nav">
          {NAV_ITEMS.map((item) => (
            <li
              key={item}
              className={`sidebar__nav-item${activePanel === item ? " sidebar__nav-item--active" : ""}`}
              onClick={() => handleNavClick(item)}
              title={item}
            >
              <span className="sidebar__nav-icon">{NAV_ICONS[item]}</span>
              <span className="sidebar__nav-label">{item}</span>
            </li>
          ))}
        </ul>
        <div className="sidebar__actions">
          <button className="sidebar__add-folder-btn" onClick={handleAddFolder}>
            + Add Folder
          </button>
        </div>
      </nav>

      {activePanel === "Folders" && <FoldersPanel onClose={closePanel} />}
      {activePanel === "Tags" && <TagsPanel onClose={closePanel} />}
      {activePanel === "Settings" && <SettingsPanel onClose={closePanel} />}

      {(activePanel === "Queue" || activePanel === "Discover") && (
        <div className="sidebar-panel-overlay" role="dialog" aria-modal="true" aria-label={activePanel}>
          <div className="sidebar-panel-overlay__backdrop" onClick={closePanel} />
          <div className="sidebar-panel-overlay__panel">
            <div className="sidebar-panel-overlay__header">
              <span className="sidebar-panel-overlay__title">{activePanel}</span>
              <button className="sidebar-panel-overlay__close" onClick={closePanel} aria-label={`Close ${activePanel}`}>✕</button>
            </div>
            <div className="sidebar-panel-overlay__body">
              {activePanel === "Queue" && <QueuePanel />}
              {activePanel === "Discover" && <DiscoveryFeed />}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
