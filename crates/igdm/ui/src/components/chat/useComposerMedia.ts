// Composer media plumbing: attach picker, clipboard-image paste and the
// "real thread only" guard. Extracted to keep the composer's control flow
// small; the view still owns the text input and edit session.

import { type ClipboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useApp } from "../../hooks/useApp";

const IMAGE_EXTS = new Set(["jpg", "jpeg", "png", "webp"]);
const VIDEO_EXTS = new Set(["mp4", "mov", "mkv", "webm"]);

export function useComposerMedia(openKey: string) {
  const { sendPhoto, sendPhotoBytes, sendVideo } = useApp();
  const virtualKey = openKey.startsWith("user:");

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

  return { onPaste, doAttach, requireRealThread };
}
