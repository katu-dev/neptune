import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMusicStore } from "../store/index";
import "./TitleBar.css";

export default function TitleBar() {
  const appWindow = getCurrentWindow();
  const searchQuery = useMusicStore((s) => s.searchQuery);
  const setSearchQuery = useMusicStore((s) => s.setSearchQuery);

  return (
    <div className="titlebar" data-tauri-drag-region>
      <input
        className="titlebar__search"
        type="search"
        placeholder="Search tracks..."
        aria-label="Search tracks"
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
      />
      <button className="titlebar__settings" aria-label="Settings">
        ⚙
      </button>
      <div className="titlebar__window-controls">
        <button
          className="titlebar__wc-btn titlebar__wc-btn--minimize"
          aria-label="Minimize"
          onClick={() => appWindow.minimize()}
        >
          ─
        </button>
        <button
          className="titlebar__wc-btn titlebar__wc-btn--maximize"
          aria-label="Maximize"
          onClick={() => appWindow.toggleMaximize()}
        >
          □
        </button>
        <button
          className="titlebar__wc-btn titlebar__wc-btn--close"
          aria-label="Close"
          onClick={() => appWindow.close()}
        >
          ✕
        </button>
      </div>
    </div>
  );
}
