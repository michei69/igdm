import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";

interface Props {
  x: number;
  y: number;
  onClose: () => void;
  children: ReactNode;
}

export default function ContextMenu({ x, y, onClose, children }: Props) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const onCloseRef = useRef(onClose);

  // Clamp to the viewport: measure after mount but before paint, then shift
  // the menu back inside when it would overflow the right or bottom edge.
  useLayoutEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const pad = 8;
    const left = Math.min(rect.left, Math.max(pad, window.innerWidth - rect.width - pad));
    const top = Math.min(rect.top, Math.max(pad, window.innerHeight - rect.height - pad));
    if (left !== rect.left) el.style.left = `${left}px`;
    if (top !== rect.top) el.style.top = `${top}px`;
  }, [x, y]);
  useEffect(() => {
    onCloseRef.current = onClose;
  });
  useEffect(() => {
    const close = () => onCloseRef.current();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    // Opening handlers call stopPropagation, so the event that opened this
    // menu never reaches these window listeners.
    window.addEventListener("mousedown", close);
    window.addEventListener("contextmenu", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("contextmenu", close);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  return (
    <div
      ref={menuRef}
      className="fixed z-40 flex min-w-[180px] flex-col rounded-lg border border-border bg-elevated p-1 shadow-2xl"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {children}
    </div>
  );
}

export function MenuItem({
  label,
  onSelect,
  danger,
  disabled,
}: {
  label: ReactNode;
  onSelect?: () => void;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      className={`w-full rounded-md px-3 py-1.5 text-left text-[12px] transition-colors duration-150 hover:bg-black/10 dark:hover:bg-white/10 ${
        danger ? "text-danger" : "text-ink2"
      } ${disabled ? "opacity-50" : ""}`}
      disabled={disabled}
      onClick={onSelect}
    >
      {label}
    </button>
  );
}
