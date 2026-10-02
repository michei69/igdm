import { useEffect, useRef, useState, type ClipboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useApp } from "../../hooks/useApp";
import { useVoiceRecorder } from "../../hooks/useVoiceRecorder";
import type { ReplyInfo } from "../../state";

const IMAGE_EXTS = new Set(["jpg", "jpeg", "png", "webp"]);
const VIDEO_EXTS = new Set(["mp4", "mov", "mkv", "webm"]);

export default function Composer() {
  const {
    state,
    sendText,
    sendPhoto,
    sendPhotoBytes,
    sendVideo,
    sendVoice,
    sendTyping,
    setReply,
    setReplyScroll,
  } = useApp();
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const prevReplyRef = useRef<ReplyInfo | null>(null);
  /** True while the backend has been told this thread is typing. */
  const typingSentRef = useRef(false);

  const openKey = state.openKey ?? "";
  const virtualKey = openKey.startsWith("user:");
  const canSend = value.trim().length > 0;

  const { recording, elapsed, toggle: toggleRecord } = useVoiceRecorder(openKey, sendVoice);

  // Media can only be sent to a thread that exists on the server; a virtual
  // `user:<pk>` thread has to be created by sending text first.
  const requireRealThread = () => {
    if (openKey.length === 0) return false;
    if (virtualKey) {
      toast("Open an existing chat to send media");
      return false;
    }
    return true;
  };

  // Starting a reply (via the message context menu) focuses the input so
  // typing can begin immediately. Fires on reply *changes* only, not on
  // thread switches that keep an existing reply.
  useEffect(() => {
    const reply = state.reply;
    const prev = prevReplyRef.current;
    prevReplyRef.current = reply;
    if (!reply || reply === prev) return;
    if (reply.threadKey !== openKey) return;
    inputRef.current?.focus();
  }, [state.reply, openKey]);

  // Typing indicator: published once when typing starts, cleared 2.5s after
  // the last keystroke. Publishing on every `value` change issued one IPC call
  // (and one IG request) per character.
  useEffect(() => {
    if (openKey.length === 0 || virtualKey) return;
    if (value.trim().length === 0) {
      if (typingSentRef.current) {
        typingSentRef.current = false;
        sendTyping(openKey, false);
      }
      return;
    }
    if (!typingSentRef.current) {
      typingSentRef.current = true;
      sendTyping(openKey, true);
    }
    const t = setTimeout(() => {
      typingSentRef.current = false;
      sendTyping(openKey, false);
    }, 2500);
    return () => clearTimeout(t);
  }, [value, openKey, virtualKey, sendTyping]);

  // Leaving a thread (or unmounting) with a pending indicator clears it, so
  // the other participant does not stay flagged as "typing" until the
  // server-side expiry.
  useEffect(() => {
    return () => {
      if (typingSentRef.current) {
        typingSentRef.current = false;
        sendTyping(openKey, false);
      }
    };
  }, [openKey, sendTyping]);

  const doSend = () => {
    const text = value.trim();
    if (text.length === 0 || openKey.length === 0) return;
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

  const sendPastedImages = async (files: File[]) => {
    if (!requireRealThread()) return;
    toast("Uploading photo…");
    await Promise.all(
      files.map(async (file) => {
        const ext = file.type === "image/jpeg" ? "jpg" : (file.type.split("/")[1] ?? "png");
        sendPhotoBytes(openKey, new Uint8Array(await file.arrayBuffer()), ext);
      }),
    );
  };

  // Paste an image from the clipboard into the input -> auto-send it.
  const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
    const files = Array.from(e.clipboardData.items).flatMap((item) => {
      if (item.kind === "file" && item.type.startsWith("image/")) {
        const f = item.getAsFile();
        return f !== null ? [f] : [];
      }
      return [];
    });
    if (files.length === 0) return;
    e.preventDefault();
    void sendPastedImages(files);
  };

  const doAttach = async () => {
    if (!requireRealThread()) return;
    // The picker runs in the backend: only paths it returns are accepted by
    // send_photo/send_video, so a compromised renderer cannot ask for an
    // arbitrary file to be uploaded.
    let file: string | null = null;
    try {
      file = await invoke<string | null>("pick_media");
    } catch (err) {
      toast(`Could not open the file picker: ${err instanceof Error ? err.message : String(err)}`);
      return;
    }
    if (!file) return;
    const ext = file.split(".").pop()?.toLowerCase() ?? "";
    if (IMAGE_EXTS.has(ext)) {
      toast("Uploading photo…");
      sendPhoto(openKey, file);
    } else if (VIDEO_EXTS.has(ext)) {
      toast("Uploading video…");
      sendVideo(openKey, file);
    } else {
      toast("Only images and videos are supported");
    }
  };

  const onRecordClick = () => {
    if (!requireRealThread()) return;
    void toggleRecord();
  };

  return (
    <div
      className="flex w-full shrink-0 items-end gap-2.5 px-4 pb-3.5 pt-2.5"
      style={{
        backgroundColor: "var(--ct-glass-bg, var(--ct-composer-bg))",
        backdropFilter: "blur(var(--ct-blur-px, 0px))",
        WebkitBackdropFilter: "blur(var(--ct-blur-px, 0px))",
      }}
    >
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
      <input
        ref={inputRef}
        className="ct-input h-9 min-w-0 flex-1 rounded-lg border px-3 text-[13.5px] outline-none"
        style={{
          backgroundColor: "var(--ct-input-bg)",
          color: "var(--ct-input-text)",
          borderColor: "var(--ct-separator)",
        }}
        placeholder="Message…"
        aria-label="Message"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onPaste={onPaste}
        onKeyDown={(e) => {
          if (e.key === "Enter") doSend();
          else if (e.key === "Escape" && state.reply && state.reply.threadKey === openKey) {
            setReplyScroll(null, 0);
            setReply(null);
          }
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
      >
        Send
      </button>
    </div>
  );
}
