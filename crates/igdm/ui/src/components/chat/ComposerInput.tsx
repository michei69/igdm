// The composer's text field: prefilled while editing, Enter saves/sends, Esc
// cancels the edit first and the reply quote second.

import type { ClipboardEvent, KeyboardEvent, RefObject } from "react";

interface Props {
  inputRef: RefObject<HTMLInputElement | null>;
  value: string;
  editing: boolean;
  hasReply: boolean;
  onChange: (value: string) => void;
  onPaste: (e: ClipboardEvent<HTMLInputElement>) => void;
  onSubmit: () => void;
  onCancelEdit: () => void;
  onCancelReply: () => void;
}

export default function ComposerInput({
  inputRef,
  value,
  editing,
  hasReply,
  onChange,
  onPaste,
  onSubmit,
  onCancelEdit,
  onCancelReply,
}: Props) {
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      onSubmit();
      return;
    }
    if (e.key !== "Escape") return;
    if (editing) {
      onCancelEdit();
      return;
    }
    if (hasReply) onCancelReply();
  };

  return (
    <input
      ref={inputRef}
      className="ct-input h-9 min-w-0 flex-1 rounded-lg border px-3 text-[13.5px] outline-none"
      style={{
        backgroundColor: "var(--ct-input-bg)",
        color: "var(--ct-input-text)",
        borderColor: "var(--ct-separator)",
      }}
      placeholder={editing ? "Edit message…" : "Message…"}
      aria-label={editing ? "Edit message" : "Message"}
      autoFocus={editing}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onPaste={onPaste}
      onKeyDown={onKeyDown}
    />
  );
}
