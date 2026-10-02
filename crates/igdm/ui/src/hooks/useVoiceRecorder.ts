import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

/** Hand the recorded bytes to the backend for `threadKey`. */
type SendVoice = (threadKey: string, bytes: Uint8Array, ext: string) => void;

/**
 * Voice-message recorder: `getUserMedia` + `MediaRecorder`, handing the
 * captured bytes to `sendVoice` when the user stops.
 *
 * `openKey` is the thread the recording belongs to: switching threads, or
 * unmounting, cancels an in-progress recording without sending it.
 */
export function useVoiceRecorder(openKey: string, sendVoice: SendVoice) {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<number | null>(null);
  const disposedRef = useRef(false);

  // Stop recording and drop the stream; returns the recorder so the caller
  // decides whether to send what was captured.
  const stopRecording = () => {
    if (timerRef.current !== null) {
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

  // Stop and discard: used when the thread changes or the hook unmounts.
  const cancel = () => {
    const rec = stopRecording();
    if (rec && rec.state !== "inactive") {
      rec.onstop = null;
      rec.stop();
    }
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

  const toggle = async () => {
    if (openKey.length === 0) return;
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
      // Guard against an unmount race: if the hook went away while awaiting,
      // stop the tracks immediately instead of leaving the mic open.
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
  // Keyed on the thread alone: with `recording` in the deps this also ran on
  // the render that starts a recording, stopping the recorder and the mic
  // tracks the moment they were created (and never sending anything).
  const prevOpenKeyRef = useRef(openKey);
  useEffect(() => {
    const previous = prevOpenKeyRef.current;
    prevOpenKeyRef.current = openKey;
    if (previous === openKey) return;
    cancel();
  }, [openKey]);

  // Cleanup on unmount. `disposedRef` is reset on mount because StrictMode
  // re-runs effects on the same instance: left set, the guard in `toggle`
  // would discard every microphone stream for the app's life.
  useEffect(() => {
    disposedRef.current = false;
    return () => {
      disposedRef.current = true;
      cancel();
    };
  }, []);

  return { recording, elapsed, toggle };
}
