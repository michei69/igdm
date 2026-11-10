import { useRef, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { useVoiceRecorder } from "../../hooks/useVoiceRecorder";
import ComposerInput from "./ComposerInput";
import EditingBanner from "./EditingBanner";
import { useComposerMedia } from "./useComposerMedia";
import { useReplyFocus } from "./useComposerFocus";
import { useTypingIndicator } from "./useTypingIndicator";

/** The message being edited; owned by MainScreen so the composer and the
 * message list can share it without adding UI state to the data layer. */
export interface EditTarget {
  threadKey: string;
  msgId: string;
  /** Original text, so an unchanged save can just cancel. */
  text: string;
  clientContext: string | null;
}

interface Props {
  editing: EditTarget | null;
  onCancelEdit: () => void;
}

/** The edit target when it belongs to the open thread, else null. */
function editFor(editing: EditTarget | null, openKey: string): EditTarget | null {
  if (!editing) return null;
  return editing.threadKey === openKey ? editing : null;
}

function hasText(value: string): boolean {
  return value.trim().length > 0;
}

export default function Composer({ editing, onCancelEdit }: Props) {
  const {
    state,
    sendText,
    sendVoice,
    sendTyping,
    setReply,
    setReplyScroll,
    updateThread,
    editMessage,
  } = useApp();
  const openKey = state.openKey ?? "";
  // MainScreen re-keys the composer per edit session, so the edit text is the
  // initial state instead of being pushed in by an effect.
  const [value, setValue] = useState(() => editFor(editing, openKey)?.text ?? "");
  const inputRef = useRef<HTMLInputElement | null>(null);

  const virtualKey = openKey.startsWith("user:");
  const editingHere = editFor(editing, openKey);
  const canSend = hasText(value);
  const sendingEdit = editingHere !== null;

  const hasReply = !!(state.reply && state.reply.threadKey === openKey);

  const { recording, elapsed, toggle: toggleRecord } = useVoiceRecorder(openKey, sendVoice);
  const { onPaste, doAttach, requireRealThread } = useComposerMedia(openKey);
  useTypingIndicator(openKey, virtualKey, value, sendTyping);
  useReplyFocus(state.reply, openKey, inputRef);

  const doSend = () => {
    const text = value.trim();
    if (text.length === 0 || openKey.length === 0) return;
    if (editingHere) {
      // Optimistic text update; the backend confirmation (`MessageEdited`)
      // replaces the same text and `is_edited` idempotently.
      if (text !== editingHere.text.trim()) {
        updateThread(editingHere.threadKey, (t) => ({
          ...t,
          messages: t.messages.map((m) =>
            m.id === editingHere.msgId ? { ...m, text, is_edited: true } : m,
          ),
        }));
        editMessage(editingHere.threadKey, editingHere.msgId, text, editingHere.clientContext);
      }
      onCancelEdit();
      setValue("");
      return;
    }
    const reply = state.reply && state.reply.threadKey === openKey ? state.reply : null;
    const replyRef = reply
      ? { message_id: reply.msgId, client_context: reply.clientContext }
      : null;
    if (virtualKey) {
      const pk = openKey.split(":")[1] ?? "";
      sendText(openKey, text, [pk], null);
    } else {
      sendText(openKey, text, [], replyRef);
    }
    setValue("");
  };

  const onRecordClick = () => {
    if (!requireRealThread()) return;
    void toggleRecord();
  };

  return (
    <div
      className="flex w-full shrink-0 flex-col"
      style={{
        backgroundColor: "var(--ct-glass-bg, var(--ct-composer-bg))",
        backdropFilter: "blur(var(--ct-blur-px, 0px))",
        WebkitBackdropFilter: "blur(var(--ct-blur-px, 0px))",
      }}
    >
      {editingHere && (
        <EditingBanner
          text={editingHere.text}
          onCancel={() => {
            onCancelEdit();
            setValue("");
          }}
        />
      )}
      <div className="flex w-full items-end gap-2.5 px-4 pb-3.5 pt-2.5">
        {!sendingEdit && (
          <>
            <button
              className="flex h-9 w-10 shrink-0 items-center justify-center rounded-lg border text-[20px]"
              style={{
                background: "var(--ct-circle-btn, var(--ct-input-bg))",
                color: "var(--ct-circle-icon, var(--ct-icon))",
                borderColor: "var(--ct-separator)",
              }}
              onClick={() => void doAttach()}
              aria-label="Attach media"
            >
              +
            </button>
            <button
              className="flex h-9 w-10 shrink-0 items-center justify-center rounded-lg border"
              style={{
                background: recording ? "#ef4444" : "var(--ct-circle-btn, var(--ct-input-bg))",
                color: recording ? "#ffffff" : "var(--ct-circle-icon, var(--ct-icon))",
                borderColor: recording ? "#ef4444" : "var(--ct-separator)",
              }}
              onClick={onRecordClick}
              aria-label={recording ? "Stop recording and send" : "Record voice message"}
              title={recording ? "Stop and send" : "Record voice message"}
            >
              {recording ? (
                <span className="text-[12px] font-medium tabular-nums">
                  {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, "0")}
                </span>
              ) : (
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <rect x="9" y="2" width="6" height="12" rx="3" />
                  <path d="M5 10v1a7 7 0 0 0 14 0v-1" />
                  <line x1="12" y1="18" x2="12" y2="22" />
                </svg>
              )}
            </button>
          </>
        )}
        <ComposerInput
          inputRef={inputRef}
          value={value}
          editing={sendingEdit}
          hasReply={hasReply}
          onChange={setValue}
          onPaste={onPaste}
          onSubmit={doSend}
          onCancelEdit={() => {
            onCancelEdit();
            setValue("");
          }}
          onCancelReply={() => {
            setReplyScroll(null, 0);
            setReply(null);
          }}
        />
        <button
          className="flex h-9 w-16 shrink-0 items-center justify-center rounded-lg text-[13px] font-medium"
          style={{
            background: "var(--ct-send-bg, var(--color-accent))",
            color: canSend ? "var(--ct-send-fg)" : "var(--ct-secondary, var(--ig-ink3))",
            opacity: canSend ? 1 : 0.45,
          }}
          disabled={!canSend}
          onClick={doSend}
          aria-label={sendingEdit ? "Save edited message" : "Send message"}
        >
          {sendingEdit ? "Save" : "Send"}
        </button>
      </div>
    </div>
  );
}
