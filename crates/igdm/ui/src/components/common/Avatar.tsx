import { hashColor, useRemoteImage } from "../../hooks/useRemoteImage";

interface Props {
  name: string;
  url?: string | null;
  size: number;
  /** Green presence indicator: a dot in the inbox, a ring in the chat header. */
  presence?: "dot" | "ring";
}

export default function Avatar({ name, url, size, presence }: Props) {
  const initial = (name.charAt(0) || "?").toUpperCase();
  const { src, onError } = useRemoteImage(url);
  const dotSize = size >= 40 ? 10 : 8;
  return (
    <div
      className="relative shrink-0"
      style={{
        width: size,
        height: size,
        borderRadius: "9999px",
        // The header ring is the IG "Active now" cue; the dot below covers the
        // inbox rows, so only one of them is requested per avatar.
        boxShadow: presence === "ring" ? "0 0 0 2px var(--ig-live)" : undefined,
      }}
    >
      <div className="h-full w-full overflow-hidden rounded-full">
        <div
          className="flex h-full w-full items-center justify-center"
          style={{ background: hashColor(name) }}
        >
          <span className="font-bold text-white" style={{ fontSize: size * 0.42 }}>
            {initial}
          </span>
        </div>
        {src && (
          <img
            src={src}
            alt=""
            draggable={false}
            className="absolute inset-0 h-full w-full rounded-full object-cover"
            onError={onError}
          />
        )}
      </div>
      {presence === "dot" && (
        <span
          aria-hidden="true"
          className="bg-live absolute right-0 bottom-0 rounded-full"
          style={{
            width: dotSize,
            height: dotSize,
            boxShadow: "0 0 0 2px var(--ct-header-bg, var(--ig-panel))",
          }}
        />
      )}
    </div>
  );
}
