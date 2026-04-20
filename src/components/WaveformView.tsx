import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useMusicStore } from "../store";
import "./WaveformView.css";

interface WaveformData {
  track_id: number;
  samples_per_channel: number[]; // peak per column — full 50k
  rms_per_column: number[];      // rms per column  — full 50k
  channels: number;
  duration_secs: number;
}

// Progressive chunk emitted every ~20s of decoded audio
interface WaveformChunk {
  track_id:     number;
  peak:         number[];
  rms:          number[];
  cols_ready:   number;  // columns populated so far
  total_cols:   number;  // final total columns
  duration_secs: number;
}

// Internal render buffer — always total_cols long, filled left-to-right
interface RenderBuffer {
  track_id:     number;
  peak:         Float32Array;
  rms:          Float32Array;
  cols_ready:   number;
  total_cols:   number;
  duration_secs: number;
}

interface PosPayload { position_secs: number; duration_secs: number; }

// Seconds visible across the full canvas width
const VISIBLE_SECS = 4.0;

// ─── Color helpers ────────────────────────────────────────────────────────────

/**
 * Amplitude-to-color mapping tuned to the app's indigo theme.
 *
 * Low  → deep violet-blue  (#3b3fa8 range) — quiet passages
 * Mid  → primary indigo    (#6366f1 range) — normal signal
 * High → bright lavender   (#c4b5fd range) — loud transients
 *
 * This keeps the CDJ-style amplitude contrast while staying on-palette.
 */
function ampToColor(amp: number, alpha = 1.0): string {
  const t = Math.min(1, Math.max(0, amp));
  let r: number, g: number, b: number;
  if (t < 0.5) {
    // deep violet-blue → primary indigo
    const u = t * 2;
    r = Math.round(59  + u * (99  - 59));   // 59 → 99
    g = Math.round(63  + u * (102 - 63));   // 63 → 102
    b = Math.round(168 + u * (241 - 168));  // 168 → 241
  } else {
    // primary indigo → bright lavender-white
    const u = (t - 0.5) * 2;
    r = Math.round(99  + u * (196 - 99));   // 99 → 196
    g = Math.round(102 + u * (181 - 102));  // 102 → 181
    b = Math.round(241 + u * (253 - 241));  // 241 → 253
  }
  return `rgba(${r},${g},${b},${alpha})`;
}

// ─── Draw ─────────────────────────────────────────────────────────────────────

function drawWaveform(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  peak: ArrayLike<number>,
  rms: ArrayLike<number>,
  totalCols: number,       // stable total — never changes for a given track
  position_secs: number,
  duration: number,
) {
  const barCount   = totalCols;
  const secsPerBar = duration / barCount;
  const pxPerSec   = width / VISIBLE_SECS;
  const pxPerBar   = secsPerBar * pxPerSec;
  const centerX    = width / 2;
  const midY       = height / 2;

  // Soft-knee compression — keeps quiet parts visible, loud parts from clipping
  const compress = (v: number) => Math.pow(Math.min(v, 1.0), 0.5);

  // Background — matches --color-bg-surface
  ctx.fillStyle = "#18181f";
  ctx.fillRect(0, 0, width, height);

  // Subtle center line
  ctx.strokeStyle = "rgba(255,255,255,0.05)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, midY);
  ctx.lineTo(width, midY);
  ctx.stroke();

  const firstBar = Math.max(0, Math.floor((position_secs - VISIBLE_SECS / 2 - secsPerBar) / secsPerBar));
  const lastBar  = Math.min(barCount - 1, Math.ceil((position_secs + VISIBLE_SECS / 2 + secsPerBar) / secsPerBar));

  const barW = Math.max(1, Math.ceil(pxPerBar));

  for (let i = firstBar; i <= lastBar; i++) {
    const barTimeSecs = i * secsPerBar;
    const x           = centerX + (barTimeSecs - position_secs) * pxPerSec;
    const isPast      = barTimeSecs < position_secs;

    const peakAmp = compress(Math.max(0, peak[i] ?? 0));
    const rmsAmp  = compress(Math.max(0, rms[i]  ?? 0));

    const peakH = peakAmp * midY * 0.92;
    const rmsH  = rmsAmp  * midY * 0.92;

    // Dim future bars slightly
    const alpha = isPast ? 1.0 : 0.55;

    // ── Outer peak envelope (thin, bright, amplitude-colored) ──────────────
    const peakColor = ampToColor(peakAmp, alpha);

    // Upper peak spike
    ctx.fillStyle = peakColor;
    ctx.fillRect(x, midY - peakH, barW, Math.max(1, peakH - rmsH));

    // Lower peak spike (mirror)
    ctx.fillRect(x, midY + rmsH, barW, Math.max(1, peakH - rmsH));

    // ── Inner RMS body (filled, slightly desaturated) ───────────────────────
    // Build a vertical gradient for the body: bright at tips, darker at center
    const bodyColorTop    = ampToColor(rmsAmp, alpha * 0.9);
    const bodyColorCenter = ampToColor(rmsAmp * 0.4, alpha * 0.5);

    if (rmsH > 1) {
      const gradUp = ctx.createLinearGradient(0, midY - rmsH, 0, midY);
      gradUp.addColorStop(0, bodyColorTop);
      gradUp.addColorStop(1, bodyColorCenter);
      ctx.fillStyle = gradUp;
      ctx.fillRect(x, midY - rmsH, barW, rmsH);

      const gradDown = ctx.createLinearGradient(0, midY, 0, midY + rmsH);
      gradDown.addColorStop(0, bodyColorCenter);
      gradDown.addColorStop(1, bodyColorTop);
      ctx.fillStyle = gradDown;
      ctx.fillRect(x, midY, barW, rmsH);
    }
  }

  // ── Edge fade-out ──────────────────────────────────────────────────────────
  const fadeW = width * 0.08;
  const gL = ctx.createLinearGradient(0, 0, fadeW, 0);
  gL.addColorStop(0, "#18181f"); gL.addColorStop(1, "transparent");
  ctx.fillStyle = gL;
  ctx.fillRect(0, 0, fadeW, height);

  const gR = ctx.createLinearGradient(width - fadeW, 0, width, 0);
  gR.addColorStop(0, "transparent"); gR.addColorStop(1, "#18181f");
  ctx.fillStyle = gR;
  ctx.fillRect(width - fadeW, 0, fadeW, height);

  // ── Playhead ───────────────────────────────────────────────────────────────
  ctx.save();
  // Outer glow
  ctx.shadowColor = "rgba(255,255,255,0.6)";
  ctx.shadowBlur  = 10;
  ctx.strokeStyle = "#ffffff";
  ctx.globalAlpha = 0.9;
  ctx.lineWidth   = 1.5;
  ctx.beginPath();
  ctx.moveTo(centerX, 0);
  ctx.lineTo(centerX, height);
  ctx.stroke();
  // Small triangle marker at top and bottom
  ctx.globalAlpha = 0.85;
  ctx.fillStyle   = "#ffffff";
  ctx.shadowBlur  = 4;
  const tri = 5;
  ctx.beginPath();
  ctx.moveTo(centerX - tri, 0);
  ctx.lineTo(centerX + tri, 0);
  ctx.lineTo(centerX, tri * 1.4);
  ctx.closePath();
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(centerX - tri, height);
  ctx.lineTo(centerX + tri, height);
  ctx.lineTo(centerX, height - tri * 1.4);
  ctx.closePath();
  ctx.fill();
  ctx.restore();
}

// ─── Component ────────────────────────────────────────────────────────────────

export default function WaveformView() {
  const selectedTrackId      = useMusicStore((s) => s.selectedTrackId);
  const waveformReadyTrackId = useMusicStore((s) => s.waveformReadyTrackId);

  const canvasRef     = useRef<HTMLCanvasElement>(null);
  const bufferRef     = useRef<RenderBuffer | null>(null);
  const loadingDivRef = useRef<HTMLDivElement>(null);
  const posRef        = useRef<PosPayload>({ position_secs: 0, duration_secs: 0 });
  const rafRef        = useRef(0);

  // Listen for playback position
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<PosPayload>("playback_position", (e) => {
      posRef.current = e.payload;
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Listen for progressive waveform chunks
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<WaveformChunk>("waveform_chunk", (e) => {
      const chunk = e.payload;
      if (chunk.track_id !== selectedTrackId) return;

      // Initialize or update the render buffer
      if (!bufferRef.current || bufferRef.current.track_id !== chunk.track_id) {
        bufferRef.current = {
          track_id:     chunk.track_id,
          peak:         new Float32Array(chunk.total_cols),
          rms:          new Float32Array(chunk.total_cols),
          cols_ready:   0,
          total_cols:   chunk.total_cols,
          duration_secs: chunk.duration_secs,
        };
      }

      // Copy the new chunk data into the buffer
      const buf = bufferRef.current;
      for (let i = 0; i < chunk.peak.length; i++) {
        buf.peak[i] = chunk.peak[i];
        buf.rms[i]  = chunk.rms[i];
      }
      buf.cols_ready = chunk.cols_ready;

      // Hide the loading spinner on first chunk
      if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [selectedTrackId]);

  const syncCanvasSize = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const w = Math.round(rect.width), h = Math.round(rect.height);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w; canvas.height = h;
    }
  }, []);

  const draw = useCallback((position_secs: number) => {
    const canvas = canvasRef.current;
    const buffer = bufferRef.current;
    if (!canvas || !buffer || buffer.cols_ready === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    
    drawWaveform(
      ctx,
      canvas.width,
      canvas.height,
      buffer.peak,
      buffer.rms,
      buffer.total_cols,       // always use full length for stable time mapping
      position_secs,
      buffer.duration_secs,
    );
  }, []);

  // RAF loop
  useEffect(() => {
    const loop = () => {
      rafRef.current = requestAnimationFrame(loop);
      syncCanvasSize();
      if (bufferRef.current && bufferRef.current.cols_ready > 0) {
        draw(posRef.current.position_secs);
      }
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
  }, [draw, syncCanvasSize]);

  useEffect(() => {
    if (selectedTrackId === null) {
      bufferRef.current = null;
      if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
      const canvas = canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext("2d");
        if (ctx) { ctx.fillStyle = "#18181f"; ctx.fillRect(0, 0, canvas.width, canvas.height); }
      }
      return;
    }

    let cancelled = false;
    bufferRef.current = null;
    if (loadingDivRef.current) loadingDivRef.current.style.display = "flex";

    invoke<WaveformData>("get_waveform", { trackId: selectedTrackId })
      .then((data) => {
        if (cancelled) return;
        // Final complete data — replace buffer
        bufferRef.current = {
          track_id:     data.track_id,
          peak:         new Float32Array(data.samples_per_channel),
          rms:          new Float32Array(data.rms_per_column),
          cols_ready:   data.samples_per_channel.length,
          total_cols:   data.samples_per_channel.length,
          duration_secs: data.duration_secs,
        };
        if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
      })
      .catch(() => {
        if (cancelled) return;
        if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
      });

    return () => { cancelled = true; };
  }, [selectedTrackId]);

  useEffect(() => {
    if (waveformReadyTrackId !== null && waveformReadyTrackId === selectedTrackId && !bufferRef.current) {
      invoke<WaveformData>("get_waveform", { trackId: selectedTrackId! })
        .then((data) => {
          bufferRef.current = {
            track_id:     data.track_id,
            peak:         new Float32Array(data.samples_per_channel),
            rms:          new Float32Array(data.rms_per_column),
            cols_ready:   data.samples_per_channel.length,
            total_cols:   data.samples_per_channel.length,
            duration_secs: data.duration_secs,
          };
          if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
        })
        .catch(() => {
          if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
        });
    }
  }, [waveformReadyTrackId, selectedTrackId]);

  const handleClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const buffer = bufferRef.current;
    if (!buffer || buffer.duration_secs <= 0) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect     = canvas.getBoundingClientRect();
    const pxPerSec = rect.width / VISIBLE_SECS;
    const offsetPx = (e.clientX - rect.left) - rect.width / 2;
    const seekSecs = Math.max(0, Math.min(buffer.duration_secs,
      posRef.current.position_secs + offsetPx / pxPerSec
    ));
    invoke("seek", { positionSecs: seekSecs }).catch(() => {});
  }, []);

  if (selectedTrackId === null) {
    return <div className="waveform-view"><div className="waveform-view__empty">No track selected</div></div>;
  }

  return (
    <div className="waveform-view">
      <canvas ref={canvasRef} className="waveform-view__canvas" onClick={handleClick} />
      <div ref={loadingDivRef} className="waveform-view__loading" style={{ display: "none" }}>
        <div className="waveform-view__spinner" />
        <span>Loading waveform…</span>
      </div>
    </div>
  );
}
