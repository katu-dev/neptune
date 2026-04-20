import { useMusicStore } from "../store/index";
import "./TagFilterBar.css";

export default function TagFilterBar() {
  const { tags, activeTagIds, setActiveTagIds } = useMusicStore();

  if (tags.length === 0) return null;

  function toggle(id: number) {
    setActiveTagIds(
      activeTagIds.includes(id)
        ? activeTagIds.filter((t) => t !== id)
        : [...activeTagIds, id]
    );
  }

  return (
    <div className="tag-filter-bar" role="toolbar" aria-label="Filter by tag">
      {activeTagIds.length > 0 && (
        <button
          className="tag-filter-bar__clear"
          onClick={() => setActiveTagIds([])}
          title="Clear tag filter"
        >
          ✕ Clear
        </button>
      )}
      {tags.map((tag) => {
        const active = activeTagIds.includes(tag.id);
        return (
          <button
            key={tag.id}
            className={`tag-filter-bar__pill${active ? " tag-filter-bar__pill--active" : ""}`}
            style={active ? { background: tag.color, borderColor: tag.color, color: "#fff" } : { borderColor: tag.color, color: tag.color }}
            onClick={() => toggle(tag.id)}
          >
            <span className="tag-filter-bar__dot" style={{ background: tag.color }} />
            {tag.name}
          </button>
        );
      })}
    </div>
  );
}
