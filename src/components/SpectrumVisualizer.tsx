/**
 * SpectrumVisualizer — Amethyst-style spectrum
 *
 * Visual approach based on Amethyst's SpectrumAnalyzer:
 * - Log-parabolic interpolation for smooth frequency mapping
 * - Filled area below the curve with vertical opacity falloff
 * - Thin bright line drawn on top of the fill
 * - Bars grow upward from the bottom
 *
 * Data: real FFT bins from Rust via `spectrum_data` event.
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMusicStore } from "../store";
import "./SpectrumVisualizer.css";

const NUM_BINS   = 128;
const ATTACK     = 0.5;
const RELEASE    = 0.03;
const LINE_W     = 1.5;
const FILL_ALPHA = 0.35;  // base fill opacity at bottom
const FALLOFF    = 0.55;  // how quickly fill fades toward top

const PR = 99, PG = 102, PB = 241;

interface SpectrumPayload { bins: number[]; rms_l: number; rms_r: number; }

/**
 * Log-parabolic interpolation — same algorithm as Amethyst's logParabolicSpectrum.
 * Maps raw FFT bins onto outputLength points with logarithmic frequency scaling
 * and parabolic interpolation for smoothness.
 */
function logParabolicSpectrum(data: Float32Array, outputLength: number): Float32Array {
  const maxIndex = data.length - 1;
  const result = new Float32Array(outputLength);
  for (let i = 0; i < outputLength; i++) {
    const logIndex = Math.pow(maxIndex, i / (outputLength - 1));
    const base = Math.floor(logIndex);
    const t = logIndex - base;
    const y0 = data[Math.max(base - 1, 0)];
    const y1 = data[base];
    const y2 = data[Math.min(base + 1, maxIndex)];
    // parabolic interpolation
    const a = (y0 - 2 * y1 + y2) / 2;
    const b = (y2 - y0) / 2;
    result[i] = Math.max(0, Math.min(1, a * t * t + b * t + y1));
  }
  return result;
}

export default function SpectrumVisualizer() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const playbackState   = useMusicStore((s) => s.playbackState);

  const canvasRef  = useRef<HTMLCanvasElement>(null);
  // Raw bins from Rust (normalised 0–1)
  const rawBins    = useRef(new Float32Array(NUM_BINS));
  // Smoothed display values
  const smoothed   = useRef(new Float32Array(NUM_BINS));
  const rafRef     = useRef(0);
  const stateRef   = useRef(playbackState);

  useEffect(() => { stateRef.current = playbackState; }, [playbackState]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<SpectrumPayload>("spectrum_data", (e) => {
      const bins = e.payload.bins;
      for (let i = 0; i < NUM_BINS && i < bins.length; i++) {
        rawBins.current[i] = bins[i];
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    if (selectedTrackId === null) {
      rawBins.current.fill(0);
    }
  }, [selectedTrackId]);

  useEffect(() => {
    const loop = () => {
      rafRef.current = requestAnimationFrame(loop);

      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const rect = canvas.getBoundingClientRect();
      const W = Math.round(rect.width), H = Math.round(rect.height);
      if (canvas.width !== W || canvas.height !== H) {
        canvas.width = W; canvas.height = H;
      }
      if (W === 0 || H === 0) return;

      const isPlaying = stateRef.current === "playing";

      // Smooth toward targets
      for (let i = 0; i < NUM_BINS; i++) {
        const target = isPlaying ? rawBins.current[i] : 0;
        const cur = smoothed.current[i];
        const rel = RELEASE + (i / (NUM_BINS - 1)) * 0.02;
        smoothed.current[i] = target > cur
          ? cur + (target - cur) * ATTACK
          : Math.max(0, cur - rel);
      }

      // Apply log-parabolic interpolation for display
      const display = logParabolicSpectrum(smoothed.current, W);

      // Clear
      ctx.clearRect(0, 0, W, H);
      ctx.fillStyle = "#0f0f13";
      ctx.fillRect(0, 0, W, H);

      if (selectedTrackId === null) {
        // Flat line when no track
        ctx.strokeStyle = `rgba(${PR},${PG},${PB},0.2)`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, H);
        ctx.lineTo(W, H);
        ctx.stroke();
        return;
      }

      // ── Filled area ──────────────────────────────────────────────────────
      // Vertical gradient: opaque at bottom, transparent at top (Amethyst style)
      const fillGrad = ctx.createLinearGradient(0, H, 0, 0);
      fillGrad.addColorStop(0,   `rgba(${PR},${PG},${PB},${FILL_ALPHA})`);
      fillGrad.addColorStop(1 - FALLOFF, `rgba(${PR},${PG},${PB},${FILL_ALPHA * 0.4})`);
      fillGrad.addColorStop(1,   `rgba(${PR},${PG},${PB},0.0)`);

      ctx.beginPath();
      ctx.moveTo(0, H);
      for (let x = 0; x < W; x++) {
        const y = H - display[x] * H;
        if (x === 0) ctx.lineTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.lineTo(W, H);
      ctx.closePath();
      ctx.fillStyle = fillGrad;
      ctx.fill();

      // ── Line on top ───────────────────────────────────────────────────────
      ctx.beginPath();
      for (let x = 0; x < W; x++) {
        const y = H - display[x] * H;
        if (x === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.strokeStyle = `rgba(${PR},${PG},${PB},0.9)`;
      ctx.lineWidth   = LINE_W;
      ctx.lineJoin    = "round";
      ctx.stroke();
    };

    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
  }, [selectedTrackId]);

  return (
    <div className="spectrum-visualizer">
      <canvas ref={canvasRef} className="spectrum-visualizer__canvas" />
    </div>
  );
}
