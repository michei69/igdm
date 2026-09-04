import { useMemo, useRef, useState, type MouseEvent, type KeyboardEvent } from "react";
import type { Attachment } from "../../lib/attachment";

const BARS = 44;

function formatTime(sec: number): string {
  const s = Math.round(sec);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

interface Props {
  att: Extract<Attachment, { kind: "voice" }>;
  own: boolean;
}

/** Inline voice message: waveform bars double as a seekable progress bar. */
export default function VoicePlayer({ att, own }: Props) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [progress, setProgress] = useState(0);
  const durationSec = Math.max(att.durationMs / 1000, 1);

  // Downsample the server waveform to a fixed bar count (peak per bucket).
  const bars = useMemo(() => {
    const data = att.waveform.length > 0 ? att.waveform : [];
    const out: number[] = [];
    for (let i = 0; i < BARS; i++) {
      if (data.length === 0) {
        out.push(0.3);
        continue;
      }
      const start = Math.floor((i * data.length) / BARS);
      const end = Math.max(start + 1, Math.floor(((i + 1) * data.length) / BARS));
      let peak = 0;
      for (let j = start; j < end; j++) peak = Math.max(peak, data[j] ?? 0);
      out.push(peak);
    }
    return out;
  }, [att.waveform]);

  const toggle = () => {
    const a = audioRef.current;
    if (!a) return;
    if (a.paused) void a.play();
    else a.pause();
  };

  const seek = (e: MouseEvent<HTMLDivElement>) => {
    const a = audioRef.current;
    if (!a) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
    a.currentTime = frac * durationSec;
    setProgress(frac);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle();
    }
  };

  const playedCls = own ? "bg-white" : "bg-accent";
  const idleCls = own ? "bg-white/30" : "bg-black/15";

  return (
    <div className="flex items-center gap-2.5 py-0.5">
      <audio
        ref={audioRef}
        src={att.audioUrl}
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => {
          setPlaying(false);
          setProgress(1);
        }}
        onTimeUpdate={(e) => {
          const a = e.currentTarget;
          if (a.duration > 0) setProgress(a.currentTime / a.duration);
        }}
      />
      <button
        type="button"
        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[11px] text-white ${
          own ? "bg-white/25" : "bg-black/30"
        }`}
        onClick={toggle}
        aria-label={playing ? "Pause voice message" : "Play voice message"}
      >
        {playing ? "❚❚" : "▶"}
      </button>
      <div
        className="flex cursor-pointer items-center gap-[2.5px]"
        onClick={seek}
        onKeyDown={handleKeyDown}
        role="slider"
        tabIndex={0}
        aria-label="Voice message progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(progress * 100)}
      >
        {bars.map((v, i) => (
          <div
            key={i}
            className={`w-[3px] rounded-full ${i / BARS <= progress ? playedCls : idleCls}`}
            style={{ height: `${Math.max(2, Math.round(v * 28))}px` }}
          />
        ))}
      </div>
      <span className={`shrink-0 text-[11px] ${own ? "text-white/70" : "text-ink2"}`}>
        {formatTime(progress * durationSec)} / {formatTime(durationSec)}
      </span>
    </div>
  );
}
