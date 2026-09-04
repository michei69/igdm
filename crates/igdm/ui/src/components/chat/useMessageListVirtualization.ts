import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ThreadState } from "../../state";
import { buildRows, type Row } from "../../lib/buildRows";

const HINT_HEIGHT = 32;
const DAY_HEIGHT = 40;
const ESTIMATED_MESSAGE_HEIGHT = 72;

/** Within this many px of the content bottom the user counts as "at the
 * bottom". Scrolling up past it unsticks; a new message/reaction then no
 * longer yanks the user down. */
const STICK_EPSILON = 8;

interface UseMessageListVirtualizationProps {
  ts: ThreadState | undefined;
  meId: string;
  openKey: string | null;
  loadOlder: (threadId: string, cursor: string) => void;
}

export function useMessageListVirtualization({
  ts,
  meId,
  openKey,
  loadOlder,
}: UseMessageListVirtualizationProps) {
  const scrollElRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(0);
  const stickRef = useRef(true);
  const scrollMemory = useRef<Map<string, { offset: number; stick: boolean }>>(new Map());
  const prevKey = useRef("");
  const pendingRestore = useRef<string | null>(null);
  const prevTotalRef = useRef(0);
  const prevOldestIdRef = useRef<string | null>(null);

  const rows: Row[] = useMemo(() => (ts ? buildRows(ts, meId) : []), [ts, meId]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElRef.current,
    estimateSize: (i) =>
      rows[i].kind === "hint"
        ? HINT_HEIGHT
        : rows[i].kind === "day"
          ? DAY_HEIGHT
          : ESTIMATED_MESSAGE_HEIGHT,
    overscan: 8,
    getItemKey: (i) => rows[i].key,
    useFlushSync: false,
  });

  const totalSize = virtualizer.getTotalSize();
  const bottomDist = Math.max(0, totalSize - (scrollTop + viewH));

  // Viewport height + scroll tracking. Stick intent is set ONLY by real
  // scrolls: content growth (a new message, a reaction growing the last row)
  // changes `scrollHeight` without firing a scroll event, so it never breaks
  // the stick — but scrolling up (even a little) does.
  useEffect(() => {
    const el = scrollElRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    const onScroll = () => {
      setScrollTop(el.scrollTop);
      const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
      stickRef.current = dist <= STICK_EPSILON;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      ro.disconnect();
      el.removeEventListener("scroll", onScroll);
    };
  }, []);

  // Per-thread scroll memory: save outgoing position, restore on return. A
  // layout effect so `pendingRestore` is set before the pin effect below.
  useLayoutEffect(() => {
    const key = openKey ?? "";
    if (!key) return;
    const prev = prevKey.current;
    const el = scrollElRef.current;
    if (prev && prev !== key && el) {
      scrollMemory.current.set(prev, { offset: el.scrollTop, stick: stickRef.current });
    }
    prevKey.current = key;
    if (prev !== key) {
      stickRef.current = true;
    }
    if (scrollMemory.current.has(key)) {
      pendingRestore.current = key;
    }
  }, [openKey]);

  // Pin to the bottom while stuck. Runs on any layout change — a new message,
  // a reaction that grows the last row and shifts the scroll up, older
  // prepends, or a resize — skipping only during a thread-switch restore so it
  // never fights the saved scroll position.
  useLayoutEffect(() => {
    const el = scrollElRef.current;
    if (!el || pendingRestore.current || !stickRef.current || rows.length === 0) return;
    el.scrollTop = Math.max(0, el.scrollHeight - el.clientHeight);
  }, [rows, totalSize, viewH]);

  // Restore saved scroll position when returning to a thread.
  useEffect(() => {
    if (pendingRestore.current && rows.length > 0) {
      const saved = scrollMemory.current.get(pendingRestore.current);
      pendingRestore.current = null;
      if (saved) {
        stickRef.current = saved.stick;
        const el = scrollElRef.current;
        if (saved.stick) {
          // Was pinned to the bottom; content may have grown since leaving,
          // so land on the new bottom rather than the stale saved offset.
          if (el) el.scrollTop = Math.max(0, el.scrollHeight - el.clientHeight);
        } else {
          virtualizer.scrollToOffset(saved.offset, { align: "auto" });
        }
      }
    }
  }, [rows, virtualizer]);

  // Scroll anchoring: when older messages are prepended while NOT stuck, keep
  // the visible content in place (the pin effect above handles the stuck case).
  useLayoutEffect(() => {
    const el = scrollElRef.current;
    if (!el || !ts) return;
    const oldestId = ts.messages[0]?.id ?? null;
    const prepended =
      ts.key === prevKey.current &&
      prevOldestIdRef.current !== null &&
      oldestId !== prevOldestIdRef.current;
    if (prepended && !stickRef.current) {
      const delta = totalSize - prevTotalRef.current;
      if (delta > 0) el.scrollTop += delta;
    }
    prevOldestIdRef.current = oldestId;
    prevTotalRef.current = totalSize;
  }, [rows, totalSize, ts, viewH]);

  // Jump-to-bottom: direct callback, no state round-trip.
  const jumpToBottom = useCallback(() => {
    stickRef.current = true;
    if (rows.length > 0) {
      virtualizer.scrollToIndex(rows.length - 1, { align: "end" });
    }
  }, [rows.length, virtualizer]);

  // Load older when scrolled into top zone.
  const loadOlderIfNeeded = useCallback(() => {
    if (!ts || !ts.has_more || !ts.oldest_cursor || ts.loading_older) return;
    const el = scrollElRef.current;
    if (!el) return;
    if (el.scrollTop < 400) {
      loadOlder(ts.key, ts.oldest_cursor);
    }
  }, [ts, loadOlder]);

  useEffect(() => {
    loadOlderIfNeeded();
  }, [scrollTop, loadOlderIfNeeded]);

  const showJump = bottomDist > 160;

  return {
    scrollElRef,
    rows,
    virtualizer,
    totalSize,
    showJump,
    jumpToBottom,
    loadOlderIfNeeded,
  };
}
