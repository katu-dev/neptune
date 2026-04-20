import "./TitleBar.css";

export default function TitleBar() {
  return (
    <div className="titlebar">
      <input
        className="titlebar__search"
        type="search"
        placeholder="Search tracks..."
        aria-label="Search tracks"
      />
      <button className="titlebar__settings" aria-label="Settings">
        ⚙
      </button>
    </div>
  );
}
