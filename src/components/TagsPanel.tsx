import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore, Tag } from "../store/index";
import "./TagsPanel.css";

interface Props { onClose: () => void; }

const PRESET_COLORS = [
  "#6366f1", "#f87171", "#34d399", "#fbbf24",
  "#60a5fa", "#f472b6", "#a78bfa", "#fb923c",
];

export default function TagsPanel({ onClose }: Props) {
  const { tags, setTags, setTrackTagMap, addToast } = useMusicStore();
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(PRESET_COLORS[0]);
  const [creating, setCreating] = useState(false);

  async function reload() {
    const t = await invoke<Tag[]>("get_tags");
    setTags(t);
    const pairs = await invoke<[number, number][]>("get_all_track_tags");
    setTrackTagMap(pairs);
  }

  useEffect(() => { reload(); }, []);

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    setCreating(true);
    try {
      await invoke("create_tag", { name, color: newColor });
      setNewName("");
      await reload();
    } catch (err) {
      addToast(`Failed to create tag: ${err}`);
    } finally {
      setCreating(false);
    }
  }

  async function handleDelete(id: number) {
    try {
      await invoke("delete_tag", { tagId: id });
      await reload();
    } catch (err) {
      addToast(`Failed to delete tag: ${err}`);
    }
  }

  return (
    <div className="tags-overlay" role="dialog" aria-modal="true" aria-label="Tags">
      <div className="tags-overlay__backdrop" onClick={onClose} />
      <div className="tags-panel">
        <div className="tags-panel__header">
          <span className="tags-panel__title">Tags</span>
          <button className="tags-panel__close" onClick={onClose} aria-label="Close">✕</button>
        </div>
        <div className="tags-panel__body">

          {/* Create new tag */}
          <section className="tags-panel__section">
            <div className="tags-panel__section-label">New Tag</div>
            <div className="tags-panel__create-row">
              <input
                className="tags-panel__name-input"
                type="text"
                placeholder="Tag name…"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                maxLength={32}
              />
              <button
                className="tags-panel__create-btn"
                onClick={handleCreate}
                disabled={creating || !newName.trim()}
              >
                Add
              </button>
            </div>
            <div className="tags-panel__color-row">
              {PRESET_COLORS.map((c) => (
                <button
                  key={c}
                  className={`tags-panel__color-swatch${newColor === c ? " tags-panel__color-swatch--active" : ""}`}
                  style={{ background: c }}
                  onClick={() => setNewColor(c)}
                  aria-label={c}
                />
              ))}
              <input
                type="color"
                className="tags-panel__color-custom"
                value={newColor}
                onChange={(e) => setNewColor(e.target.value)}
                title="Custom color"
              />
            </div>
          </section>

          {/* Existing tags */}
          <section className="tags-panel__section">
            <div className="tags-panel__section-label">All Tags</div>
            {tags.length === 0 ? (
              <div className="tags-panel__empty">No tags yet.</div>
            ) : (
              <ul className="tags-panel__list">
                {tags.map((tag) => (
                  <li key={tag.id} className="tags-panel__item">
                    <span className="tags-panel__dot" style={{ background: tag.color }} />
                    <span className="tags-panel__tag-name">{tag.name}</span>
                    <button
                      className="tags-panel__delete-btn"
                      onClick={() => handleDelete(tag.id)}
                      title="Delete tag"
                    >
                      ✕
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

        </div>
      </div>
    </div>
  );
}
