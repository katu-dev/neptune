/**
 * OscilloscopeVisualizer — XY vectorscope (Lissajous), Amethyst-style.
 *
 * Plots Left channel on X axis vs Right channel on Y axis.
 * Mono signals produce a diagonal line; stereo content produces a Lissajous figure.
 * Rotated 45° so a mono signal appears as a vertical line (classic vectorscope orientation).
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMusicStore } from "../store";
import "./OscilloscopeVisualizer.css";

interface SpectrumPayload {
  bins: number[];
  rms_l: number;
  rms_r: number;
  scope: number[]; // interleaved L/R, 256 frames × 2
}

const DECAY = 0.85; // trail fade per frame
const SCOPE_FRAMES = 256;

// Primary indigo accent — matches --color-primary
const R = 99, G = 102, B = 241;

export default function OscilloscopeVisualizer() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const playbackState   = useMusicStore((s) => s.playbackState);

  const canvasRef   = useRef<HTMLCanvasElement>(null);
  const scopeRef    = useRef<Float32Array>(new Float32Array(SCOPE_FRAMES * 2));
  const rafRef      = useRef(0);
  const stateRef    = useRef(playbackState);

  useEffect(() => { stateRef.current = playbackState; }, [playbackState]);

  useEffect(() => {
    if (selectedTrackId === null) scopeRef.current.fill(0);
  }, [selectedTrackId]);

  // Receive scope data from Rust
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<SpectrumPayload>("spectrum_data", (e) => {
      const s = e.payload.scope;
      if (!s || s.length < SCOPE_FRAMES * 2) return;
      for (let i = 0; i < SCOPE_FRAMES * 2; i++) {
        scopeRef.current[i] = s[i];
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    const loop = () => {
      rafRef.current = requestAnimationFrame(loop);

      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      // Sync canvas resolution to CSS size
      const rect = canvas.getBoundingClientRect();
      const W = Math.round(rect.width);
      const H = Math.round(rect.height);
      if (canvas.width !== W || canvas.height !== H) {
        canvas.width = W;
        canvas.height = H;
      }
      if (W === 0 || H === 0) return;

      const isPlaying = stateRef.current === "playing";

      // Fade trail — semi-transparent fill instead of clearRect gives the glow trail
      ctx.fillStyle = `rgba(15, 15, 19, ${DECAY})`;
      ctx.fillRect(0, 0, W, H);

      if (!isPlaying || selectedTrackId === null) {
        // Draw a faint center cross when idle
        ctx.strokeStyle = `rgba(${R},${G},${B},0.08)`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(W / 2, 0); ctx.lineTo(W / 2, H);
        ctx.moveTo(0, H / 2); ctx.lineTo(W, H / 2);
        ctx.stroke();
        return;
      }

      const cx = W / 2;
      const cy = H / 2;
      // Scale so ±1.0 maps to ~45% of half-dimension (leaves margin)
      const scaleX = cx * 0.88;
      const scaleY = cy * 0.88;

      // Draw the Lissajous path
      // Rotate 45° so mono (L=R) → vertical line
      ctx.beginPath();
      for (let i = 0; i < SCOPE_FRAMES; i++) {
        const l = scopeRef.current[i * 2];
        const r = scopeRef.current[i * 2 + 1];

        // XY vectorscope: rotate 45°
        const x = cx + (l - r) * scaleX * 0.707;
        const y = cy - (l + r) * scaleY * 0.707;

        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }

      ctx.strokeStyle = `rgba(${R},${G},${B},0.85)`;
      ctx.lineWidth = 1.2;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.stroke();

      // Bright dot at current position
      const lastL = scopeRef.current[(SCOPE_FRAMES - 1) * 2];
      const lastR = scopeRef.current[(SCOPE_FRAMES - 1) * 2 + 1];
      const dotX = cx + (lastL - lastR) * scaleX * 0.707;
      const dotY = cy - (lastL + lastR) * scaleY * 0.707;
      ctx.beginPath();
      ctx.arc(dotX, dotY, 1.5, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(${R + 100},${G + 80},${B + 12},0.95)`;
      ctx.fill();
    };

    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
  }, [selectedTrackId]);

  return (
    <div className="oscilloscope">
      <canvas ref={canvasRef} className="oscilloscope__canvas" />
    </div>
  );
}
