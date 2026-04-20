import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMusicStore } from "../store";
import "./SpectrogramView.css";

interface SpectrogramData {
  track_id: number;
  magnitudes: number[][];  // [time_frame][freq_bin], values in dB
  fft_size: number;
  hop_size: number;
  sample_rate: number;
  duration_secs: number;
}

// Inferno color map: black → purple → orange → yellow
// 256-entry LUT sampled from the matplotlib inferno palette
function infernoColor(t: number): [number, number, number] {
  // Clamp t to [0, 1]
  const v = Math.max(0, Math.min(1, t));

  // Key stops of the inferno palette (r, g, b) at t = 0, 0.25, 0.5, 0.75, 1.0
  const stops: [number, number, number][] = [
    [0,   0,   4],    // 0.00 — near black
    [87,  16,  110],  // 0.25 — deep purple
    [188, 55,  84],   // 0.50 — crimson/orange-red
    [249, 142, 9],    // 0.75 — orange
    [252, 255, 164],  // 1.00 — pale yellow
  ];

  const seg = v * (stops.length - 1);
  const lo = Math.floor(seg);
  const hi = Math.min(lo + 1, stops.length - 1);
  const f = seg - lo;

  const r = Math.round(stops[lo][0] + f * (stops[hi][0] - stops[lo][0]));
  const g = Math.round(stops[lo][1] + f * (stops[hi][1] - stops[lo][1]));
  const b = Math.round(stops[lo][2] + f * (stops[hi][2] - stops[lo][2]));
  return [r, g, b];
}

export default function SpectrogramView() {
  const selectedTrackId = useMusicStore((s) => s.selectedTrackId);
  const spectrogramReadyTrackId = useMusicStore((s) => s.spectrogramReadyTrackId);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const spectrogramRef = useRef<SpectrogramData | null>(null);
  const loadingDivRef = useRef<HTMLDivElement>(null);

  // Render the spectrogram heatmap onto the canvas
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const data = spectrogramRef.current;
    if (!canvas || !data || data.magnitudes.length === 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { width, height } = canvas;
    const timeFrames = data.magnitudes.length;
    const freqBins = data.magnitudes[0].length;

    // Find dB range for normalization
    let minDb = Infinity;
    let maxDb = -Infinity;
    for (let t = 0; t < timeFrames; t++) {
      for (let f = 0; f < freqBins; f++) {
        const v = data.magnitudes[t][f];
        if (v < minDb) minDb = v;
        if (v > maxDb) maxDb = v;
      }
    }
    const dbRange = maxDb - minDb || 1;

    // Draw pixel-by-pixel using ImageData for performance
    const imageData = ctx.createImageData(width, height);
    const pixels = imageData.data;

    for (let px = 0; px < width; px++) {
      // Map pixel x → time frame
      const tIdx = Math.floor((px / width) * timeFrames);
      const frame = data.magnitudes[Math.min(tIdx, timeFrames - 1)];

      for (let py = 0; py < height; py++) {
        // Map pixel y → freq bin (0 Hz at bottom → Nyquist at top)
        const fIdx = Math.floor(((height - 1 - py) / height) * freqBins);
        const db = frame[Math.min(fIdx, freqBins - 1)];
        const t = (db - minDb) / dbRange;
        const [r, g, b] = infernoColor(t);

        const i = (py * width + px) * 4;
        pixels[i]     = r;
        pixels[i + 1] = g;
        pixels[i + 2] = b;
        pixels[i + 3] = 255;
      }
    }

    ctx.putImageData(imageData, 0, 0);
  }, []);

  // Sync canvas resolution to its CSS size
  const syncCanvasSize = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    if (canvas.width !== rect.width || canvas.height !== rect.height) {
      canvas.width = Math.round(rect.width);
      canvas.height = Math.round(rect.height);
    }
  }, []);

  // Load spectrogram when track selection changes
  useEffect(() => {
    if (selectedTrackId === null) {
      spectrogramRef.current = null;
      if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
      const canvas = canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.clearRect(0, 0, canvas.width, canvas.height);
        }
      }
      return;
    }

    let cancelled = false;
    spectrogramRef.current = null;
    if (loadingDivRef.current) loadingDivRef.current.style.display = "flex";

    invoke<SpectrogramData>("get_spectrogram", {
      trackId: selectedTrackId,
      fftSize: null,
      hopSize: null,
    })
      .then((data) => {
        if (cancelled) return;
        spectrogramRef.current = data;
        if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
        syncCanvasSize();
        draw();
      })
      .catch(() => {
        if (cancelled) return;
        if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
      });

    return () => {
      cancelled = true;
    };
  }, [selectedTrackId, draw, syncCanvasSize]);

  // Re-fetch if spectrogram_ready fires for the currently selected track
  useEffect(() => {
    if (
      spectrogramReadyTrackId !== null &&
      spectrogramReadyTrackId === selectedTrackId &&
      !spectrogramRef.current
    ) {
      invoke<SpectrogramData>("get_spectrogram", {
        trackId: selectedTrackId,
        fftSize: null,
        hopSize: null,
      })
        .then((data) => {
          spectrogramRef.current = data;
          if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
          syncCanvasSize();
          draw();
        })
        .catch(() => {
          if (loadingDivRef.current) loadingDivRef.current.style.display = "none";
        });
    }
  }, [spectrogramReadyTrackId, selectedTrackId, draw, syncCanvasSize]);

  // Click-to-seek handler
  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const data = spectrogramRef.current;
      if (!data || data.duration_secs <= 0) return;

      const canvas = canvasRef.current;
      if (!canvas) return;

      const rect = canvas.getBoundingClientRect();
      const normalizedPos = (e.clientX - rect.left) / rect.width;
      const seekSecs = Math.max(0, Math.min(1, normalizedPos)) * data.duration_secs;

      invoke("seek", { positionSecs: seekSecs }).catch(() => {});
    },
    []
  );

  if (selectedTrackId === null) {
    return (
      <div className="spectrogram-view">
        <div className="spectrogram-view__empty">No track selected</div>
      </div>
    );
  }

  return (
    <div className="spectrogram-view">
      <canvas
        ref={canvasRef}
        className="spectrogram-view__canvas"
        onClick={handleClick}
      />
      <div
        ref={loadingDivRef}
        className="spectrogram-view__loading"
        style={{ display: "none" }}
      >
        <div className="spectrogram-view__spinner" />
        <span>Computing spectrogram…</span>
      </div>
    </div>
  );
}
