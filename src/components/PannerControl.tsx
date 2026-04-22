import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { useMusicStore } from "../store/index";
import "./PannerControl.css";

export default function PannerControl() {
  const panValue = useMusicStore((s) => s.panValue);
  const setPanValue = useMusicStore((s) => s.setPanValue);

  // Load persisted pan value from backend on mount
  useEffect(() => {
    invoke<number>("get_pan")
      .then((v) => setPanValue(v))
      .catch(() => {});
  }, []);

  function handleChange(e: React.ChangeEvent<HTMLInputElement>) {
    const value = parseFloat(e.target.value);
    setPanValue(value);
    invoke("set_pan", { value }).catch(() => {});
  }

  function handleReset() {
    setPanValue(0);
    invoke("set_pan", { value: 0 }).catch(() => {});
  }

  return (
    <div className="panner-control" aria-label="Panner">
      <div className="panner-control__row">
        <span className="panner-control__label">L</span>
        <input
          type="range"
          className="panner-control__slider"
          min={-1}
          max={1}
          step={0.01}
          value={panValue}
          onChange={handleChange}
          aria-label="Pan"
          aria-valuemin={-1}
          aria-valuemax={1}
          aria-valuenow={panValue}
        />
        <span className="panner-control__label">R</span>
      </div>
      <div className="panner-control__footer">
        <span className="panner-control__value">
          {panValue === 0
            ? "C"
            : panValue > 0
            ? `R ${panValue.toFixed(2)}`
            : `L ${Math.abs(panValue).toFixed(2)}`}
        </span>
        <button
          className="settings-btn panner-reset-btn"
          onClick={handleReset}
          aria-label="Reset pan to center"
        >
          Center
        </button>
      </div>
    </div>
  );
}
