// Bubble send pop-in: ids of messages appended while the list is mounted.
// Scroll-driven mounts are not "fresh", and the server echo that replaces an
// optimistic `local:` row must not pop the same bubble a second time.

import { useEffect, useRef, useState } from "react";
import type { DirectMessage, ThreadState } from "../../state";

/** Pop lifetime; must match `.send-pop` in styles.css. */
const FRESH_MS = 420;

export function useFreshMessages(ts: ThreadState | undefined): Set<string> {
  const [fresh, setFresh] = useState<Set<string>>(new Set());
  const known = useRef<{ key: string; messages: DirectMessage[] }>({ key: "", messages: [] });
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  useEffect(() => {
    if (!ts) return;
    const prev = known.current;
    if (prev.key !== ts.key) {
      known.current = { key: ts.key, messages: ts.messages };
      setFresh(new Set());
      return;
    }
    const knownIds = new Set(prev.messages.map((m) => m.id));
    const added = ts.messages.filter((m) => !knownIds.has(m.id));
    known.current = { key: ts.key, messages: ts.messages };
    // Bulk history pages (more than two rows at once) must not pop.
    if (added.length === 0 || added.length > 2) return;
    const localTexts = new Set(
      prev.messages.filter((m) => m.id.startsWith("local:")).map((m) => (m.text ?? "").trim()),
    );
    const popping = added.filter((m) => !localTexts.has((m.text ?? "").trim()));
    if (popping.length === 0) return;
    setFresh(new Set(popping.map((m) => m.id)));
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setFresh(new Set()), FRESH_MS);
  }, [ts]);

  return fresh;
}
