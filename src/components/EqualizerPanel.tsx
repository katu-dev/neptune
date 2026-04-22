import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { useMusicStore } from "../store/index";
import "./EqualizerPanel.css";

const EQ_BANDS = [
  { label: "60 Hz",  freq: "60 Hz"  },
  { label: "170 Hz", freq: "170 Hz" },
  { label: "310 Hz", freq: "310 Hz" },
  { label: "600 Hz", freq: "600 Hz" },
  { label: "1 kHz",  freq: "1 kHz"  },
  { label: "3 kHz",  freq: "3 kHz"  },
  { label: "6 kHz",  freq: "6 kHz"  },
  { label: "14 kHz", freq: "14 kHz" },
];

export default function EqualizerPanel() {
  const eqGains = useMusicStore((s) => s.eqGains);
  const eqBypassed = useMusicStore((s) => s.eqBypassed);
  const setEqGains = useMusicStore((s) => s.setEqGains);
  const setEqBypassed = useMusicStore((s) => s.setEqBypassed);

  // Load persisted EQ gains from backend on mount
  useEffect(() => {
    invoke<number[]>("get_eq_gains")
      .then((gains) => setEqGains(Array.from(gains)))
      .catch(() => {});
  }, []);

  function handleGainChange(band: number, value: number) {
    const next = eqGains.map((g, i) => (i === band ? value : g));
    setEqGains(next);
    invoke("set_eq_gain", { band, gainDb: value }).catch(() => {});
  }

  function handleBypassToggle() {
    const next = !eqBypassed;
    setEqBypassed(next);
    invoke("set_eq_bypassed", { bypassed: next }).catch(() => {});
  }

  function handleReset() {
    const zeros = new Array(8).fill(0);
    setEqGains(zeros);
    invoke("reset_eq").catch(() => {});
  }

  return (
    <div className="eq-panel" aria-label="Equalizer">
      <div className="eq-panel__controls">
        <label className="eq-bypass-toggle">
          <input
            type="checkbox"
            checked={eqBypassed}
            onChange={handleBypassToggle}
            aria-label="Bypass equalizer"
          />
          <span>Bypass</span>
        </label>
        <button
          className="settings-btn eq-reset-btn"
          onClick={handleReset}
          aria-label="Reset equalizer"
        >
          Reset
        </button>
      </div>

      <div className={`eq-panel__sliders${eqBypassed ? " eq-panel__sliders--bypassed" : ""}`}>
        {EQ_BANDS.map((band, i) => (
          <div key={band.freq} className="eq-band">
            <span className="eq-band__gain">
              {eqGains[i] > 0 ? "+" : ""}{eqGains[i].toFixed(1)}
            </span>
            <input
              type="range"
              className="eq-band__slider"
              min={-12}
              max={12}
              step={0.5}
              value={eqGains[i]}
              onChange={(e) => handleGainChange(i, parseFloat(e.target.value))}
              disabled={eqBypassed}
              aria-label={`${band.label} gain`}
              aria-valuemin={-12}
              aria-valuemax={12}
              aria-valuenow={eqGains[i]}
              // @ts-expect-error: 'orient' is a non-standard but widely supported attribute for vertical sliders
              orient="vertical"
            />
            <span className="eq-band__label">{band.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
