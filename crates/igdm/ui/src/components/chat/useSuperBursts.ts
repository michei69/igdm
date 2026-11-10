// Big super-emoji overlays: one per message, deduped per reaction
// (sender + emoji), seeded on thread open so loaded history stays silent.

import { useCallback, useEffect, useRef, useState } from "react";
import type { DirectMessage, ThreadState } from "../../state";
import type { SuperBurst } from "./MessageRow";
import { isSuperReaction, superKeyFor } from "./superReact";

/** Overlay state lifetime: 200 ms emission + 1.2 s particle travel. */
const SUPER_BURST_MS = 1650;

export function useSuperBursts(ts: ThreadState | undefined, meId: string) {
  const [bursts, setBursts] = useState<Record<string, SuperBurst>>({});
  const nonce = useRef(0);
  const seen = useRef<Set<string>>(new Set());
  const threadKey = useRef<string | null>(null);

  const showBurst = useCallback((msgId: string, emoji: string) => {
    const next = ++nonce.current;
    setBursts((prev) => ({ ...prev, [msgId]: { emoji, nonce: next } }));
  }, []);

  // One cleanup path owns all overlay timers: whenever the burst map changes,
  // every still-visible overlay gets a fresh lifetime (and a re-send retriggers
  // instead of stacking timers forever).
  useEffect(() => {
    const timers: number[] = [];
    for (const id of Object.keys(bursts)) {
      timers.push(
        window.setTimeout(() => {
          setBursts((prev) => {
            if (!prev[id]) return prev;
            const next = { ...prev };
            delete next[id];
            return next;
          });
        }, SUPER_BURST_MS),
      );
    }
    return () => {
      for (const timer of timers) window.clearTimeout(timer);
    };
  }, [bursts]);

  // Seed the dedupe set when the thread opens, then burst once per newly seen
  // super reaction. Removing a reaction drops its key, so re-adding replays.
  useEffect(() => {
    if (!ts) return;
    if (threadKey.current !== ts.key) {
      threadKey.current = ts.key;
      const initial = new Set<string>();
      for (const m of ts.messages) {
        for (const r of m.reactions?.emojis ?? []) {
          if (isSuperReaction(r)) initial.add(superKeyFor(m.id, String(r.sender_id), r.emoji));
        }
      }
      seen.current = initial;
      return;
    }
    const current = new Set<string>();
    for (const m of ts.messages) {
      for (const r of m.reactions?.emojis ?? []) {
        if (!isSuperReaction(r)) continue;
        const key = superKeyFor(m.id, String(r.sender_id), r.emoji);
        current.add(key);
        if (seen.current.has(key)) continue;
        showBurst(m.id, r.emoji);
      }
    }
    seen.current = current;
  }, [ts, showBurst]);

  // Own send: burst immediately and remember the reaction so the server echo
  // cannot replay the animation.
  const triggerOwn = useCallback(
    (msg: DirectMessage, emoji: string) => {
      const sender = String(Number(meId) || 0);
      seen.current.add(superKeyFor(msg.id, sender, emoji));
      showBurst(msg.id, emoji);
    },
    [meId, showBurst],
  );

  return { bursts, triggerOwn };
}
