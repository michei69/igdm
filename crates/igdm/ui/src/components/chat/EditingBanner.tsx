// IG-style "Editing message" banner shown above the composer input.

interface Props {
  /** Original text previewed so the user knows what is being edited. */
  text: string;
  onCancel: () => void;
}

export default function EditingBanner({ text, onCancel }: Props) {
  return (
    <div
      className="edit-banner flex items-center gap-2 border-t px-4 pt-2 pb-1"
      style={{ borderColor: "var(--ct-separator)" }}
    >
      <div className="min-w-0 flex-1">
        <div
          className="text-[10px] font-semibold tracking-wide uppercase"
          style={{ color: "var(--ct-emphasis, var(--color-accent))" }}
        >
          Editing message
        </div>
        <div className="truncate text-[12px]" style={{ color: "var(--ct-secondary)" }}>
          {text}
        </div>
      </div>
      <button
        type="button"
        className="shrink-0 rounded-md px-1.5 py-0.5 text-[13px] text-ink3 transition-colors duration-150 hover:bg-panel2"
        aria-label="Cancel editing"
        title="Cancel editing (Esc)"
        onClick={onCancel}
      >
        ✕
      </button>
    </div>
  );
}
