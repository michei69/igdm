import { useEffect, useRef, useState, type ClipboardEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { useApp } from "../../hooks/useApp";
import type { ReplyInfo } from "../../state";

const IMAGE_EXTS = ["jpg", "jpeg", "png", "webp"];
const VIDEO_EXTS = ["mp4", "mov", "mkv", "webm"];

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
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const prevReplyRef = useRef<ReplyInfo | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<number | null>(null);
  const disposedRef = useRef(false);

  const openKey = state.openKey ?? "";
  const virtualKey = openKey.startsWith("user:");
  const canSend = value.trim().length > 0;

  // Stop recording and drop the stream; returns the recorder so the caller
  // decides whether to send what was captured.
  const stopRecording = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    setElapsed(0);
    setRecording(false);
    const rec = recorderRef.current;
    recorderRef.current = null;
    streamRef.current?.getTracks().forEach((t) => t.stop());
    streamRef.current = null;
    return rec;
  };

  // Finish a recording: stop it and send the captured bytes. The webview
  // records webm/opus (or mp4 when the webview supports it); the backend
  // transcodes to m4a before upload.
  const sendRecorded = (rec: MediaRecorder) => {
    rec.onstop = () => {
      const blob = new Blob(chunksRef.current, { type: rec.mimeType });
      chunksRef.current = [];
      if (blob.size === 0) return;
      const ext = rec.mimeType.includes("mp4") ? "mp4" : "webm";
      void blob.arrayBuffer().then((buf) => sendVoice(openKey, new Uint8Array(buf), ext));
    };
    if (rec.state !== "inactive") rec.stop();
  };

  const toggleRecord = async () => {
    if (openKey.length === 0) return;
    if (virtualKey) {
      toast("Open an existing chat to send media");
      return;
    }
    if (recording) {
      const rec = stopRecording();
      if (rec) sendRecorded(rec);
      return;
    }
    if (typeof MediaRecorder === "undefined") {
      toast("Voice recording is not supported in this webview");
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      // Guard against unmount race: if component unmounted while awaiting, stop tracks immediately.
      if (disposedRef.current) {
        stream.getTracks().forEach((t) => t.stop());
        return;
      }
      streamRef.current = stream;
      const mimeType = MediaRecorder.isTypeSupported("audio/mp4;codecs=mp4a.40.2")
        ? "audio/mp4;codecs=mp4a.40.2"
        : "audio/webm;codecs=opus";
      const rec = new MediaRecorder(stream, mimeType ? { mimeType } : undefined);
      recorderRef.current = rec;
      chunksRef.current = [];
      rec.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };
      rec.start();
      setRecording(true);
      setElapsed(0);
      timerRef.current = setInterval(() => setElapsed((s) => s + 1), 1000);
    } catch (err) {
      streamRef.current?.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
      toast(`Mic unavailable: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  // Switching threads cancels an in-progress recording without sending.
  useEffect(() => {
    if (!recording) return;
    stopRecording()?.stop();
  }, [openKey, recording]);

  // Cleanup on unmount.
  useEffect(() => {
    return () => {
      disposedRef.current = true;
      const rec = stopRecording();
      if (rec && rec.state !== "inactive") {
        rec.onstop = null;
        rec.stop();
      }
    };
  }, []);

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

  // Typing indicator: active while typing, stops 2.5s after the last key.
  useEffect(() => {
    if (openKey.length === 0 || virtualKey) return;
    if (value.trim().length === 0) {
      sendTyping(openKey, false);
      return;
    }
    sendTyping(openKey, true);
    const t = setTimeout(() => sendTyping(openKey, false), 2500);
    return () => clearTimeout(t);
  }, [value, openKey, virtualKey, sendTyping]);

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

  const sendPastedImages = async (files: File[]) => {
    if (openKey.length === 0) return;
    if (virtualKey) {
      toast("Open an existing chat to send media");
      return;
    }
    toast("Uploading photo…");
    await Promise.all(
      files.map(async (file) => {
        const ext = file.type === "image/jpeg" ? "jpg" : (file.type.split("/")[1] ?? "png");
        sendPhotoBytes(openKey, new Uint8Array(await file.arrayBuffer()), ext);
      }),
    );
  };

  const doAttach = async () => {
    if (openKey.length === 0) return;
    const file = await open({
      multiple: false,
      filters: [{ name: "Images and videos", extensions: [...IMAGE_EXTS, ...VIDEO_EXTS] }],
    });
    if (!file) return;
    if (virtualKey) {
      toast("Open an existing chat to send media");
      return;
    }
    const ext = file.split(".").pop()?.toLowerCase() ?? "";
    if (IMAGE_EXTS.includes(ext)) {
      toast("Uploading photo…");
      sendPhoto(openKey, file);
    } else if (VIDEO_EXTS.includes(ext)) {
      toast("Uploading video…");
      sendVideo(openKey, file);
    } else {
      toast("Only images and videos are supported");
    }
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
        onClick={() => void toggleRecord()}
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
