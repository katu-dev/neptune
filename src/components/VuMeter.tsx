/**
 * VuMeter — stereo L/R bars driven by real per-channel RMS from Rust.
 * rms_l and rms_r are emitted alongside the FFT bins in spectrum_data.
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMusicStore } from "../store";
import "./VuMeter.css";

interface SpectrumPayload { bins: number[]; rms_l: number; rms_r: number; }

const SEGMENTS: [number, number, string][] = [
  [0.00, 0.60, "#34d399"],
  [0.60, 0.80, "#fbbf24"],
  [0.80, 0.92, "#f97316"],
  [0.92, 1.00, "#f87171"],
];
const GAP     = 3;
const ATTACK  = 0.75;
const DECAY   = 0.04;

function drawBar(
  ctx: CanvasRenderingContext2D,
  x: number, barW: number, H: number,
  level: number, peak: number
) {
  ctx.fillStyle = "#0f0f13";
  ctx.fillRect(x, 0, barW, H);

  for (const [from, to, color] of SEGMENTS) {
    const sBot = H - from * H;
    const sTop = H - to * H;
    const litTop = H - level * H;

    const dTop = Math.max(sTop, litTop), dBot = Math.min(sBot, H);
    if (dBot > dTop) { ctx.fillStyle = color; ctx.fillRect(x, dTop, barW, dBot - dTop); }

    const dimTop = Math.max(sTop, H), dimBot = sBot;
    if (dimBot > dimTop) {
      ctx.fillStyle = color; ctx.globalAlpha = 0.12;
      ctx.fillRect(x, dimTop, barW, dimBot - dimTop);
      ctx.globalAlpha = 1;
    }
  }

  if (peak > 0.02) {
    const py = H - peak * H;
    ctx.fillStyle = peak > 0.92 ? "#f87171" : peak > 0.80 ? "#fbbf24" : "#34d399";
    ctx.fillRect(x, py - 1, barW, 2);
  }
}

export default function VuMeter() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const playbackState   = useMusicStore((s) => s.playbackState);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef  = useRef(playbackState);
  const levelL    = useRef(0), levelR = useRef(0);
  const peakL     = useRef(0), peakR  = useRef(0);
  const peakTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rafRef    = useRef(0);
  const rawL      = useRef(0), rawR = useRef(0);

  useEffect(() => { stateRef.current = playbackState; }, [playbackState]);

  useEffect(() => {
    if (selectedTrackId === null) {
      rawL.current = rawR.current = 0;
      levelL.current = levelR.current = 0;
      peakL.current = peakR.current = 0;
    }
  }, [selectedTrackId]);

  // Real per-channel RMS from Rust
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<SpectrumPayload>("spectrum_data", (e) => {
      rawL.current = e.payload.rms_l;
      rawR.current = e.payload.rms_r;
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    const loop = () => {
      rafRef.current = requestAnimationFrame(loop);

      const isPlaying = stateRef.current === "playing";
      const lA = isPlaying ? Math.min(1, rawL.current) : 0;
      const rA = isPlaying ? Math.min(1, rawR.current) : 0;

      levelL.current = lA > levelL.current
        ? levelL.current + (lA - levelL.current) * ATTACK
        : Math.max(0, levelL.current - DECAY);
      levelR.current = rA > levelR.current
        ? levelR.current + (rA - levelR.current) * ATTACK
        : Math.max(0, levelR.current - DECAY);

      if (lA > peakL.current) {
        peakL.current = lA;
        if (peakTimer.current) clearTimeout(peakTimer.current);
        peakTimer.current = setTimeout(() => { peakL.current = 0; peakR.current = 0; }, 1500);
      }
      if (rA > peakR.current) peakR.current = rA;

      const canvas = canvasRef.current;
      if (!canvas) return;

      // Sync canvas height to its CSS-rendered height
      const rect = canvas.getBoundingClientRect();
      const H = Math.round(rect.height);
      const W = Math.round(rect.width);
      if (canvas.height !== H || canvas.width !== W) {
        canvas.height = H;
        canvas.width  = W;
      }

      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const barW = Math.floor((W - GAP) / 2);
      drawBar(ctx, 0,          barW, H, levelL.current, peakL.current);
      drawBar(ctx, barW + GAP, barW, H, levelR.current, peakR.current);
    };

    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
  }, []);

  return (
    <div className="vu-meter" aria-hidden="true">
      <canvas ref={canvasRef} className="vu-meter__canvas" />
      <div className="vu-meter__labels"><span>L</span><span>R</span></div>
    </div>
  );
}
