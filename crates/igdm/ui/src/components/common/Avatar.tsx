import { hashColor, useRemoteImage } from "../../hooks/useRemoteImage";

interface Props {
  name: string;
  url?: string | null;
  size: number;
}

export default function Avatar({ name, url, size }: Props) {
  const initial = (name.charAt(0) || "?").toUpperCase();
  const { src, onError } = useRemoteImage(url);
  return (
    <div
      className="relative shrink-0 select-none overflow-hidden rounded-full"
      style={{ width: size, height: size }}
    >
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
  );
}
