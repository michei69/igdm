import { useCallback } from "react";
import type { ThreadState } from "../state";
import { useApp } from "./useApp";

/**
 * Scroll to a replied-to message; if it isn't loaded yet, mark the thread
 * loading and fetch older history (the reducer keeps hunting on OlderLoaded).
 */
export function useReplyHunt(): (ts: ThreadState, target: string) => void {
  const { setReplyScroll, updateThread, loadOlder } = useApp();
  return useCallback(
    (ts: ThreadState, target: string) => {
      const exists = ts.messages.some((m) => m.id === target);
      setReplyScroll(target, exists ? 0 : 5);
      if (!exists && ts.oldest_cursor) {
        updateThread(ts.key, (t) => ({ ...t, loading_older: true }));
        loadOlder(ts.key, ts.oldest_cursor);
      }
    },
    [setReplyScroll, updateThread, loadOlder],
  );
}
