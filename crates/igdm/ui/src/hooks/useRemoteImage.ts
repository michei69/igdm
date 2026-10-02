import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Bounded cache: entries refresh to the tail on read and evict from the head
// once the cap is exceeded, so data URLs cannot accumulate for the session.
const MAX_CACHE_ENTRIES = 150;
const cache = new Map<string, string>();
const pending = new Map<string, Promise<string | null>>();

function cacheGet(url: string): string | undefined {
  const data = cache.get(url);
  if (data !== undefined) {
    cache.delete(url);
    cache.set(url, data);
  }
  return data;
}

function cacheSet(url: string, data: string): void {
  cache.delete(url);
  cache.set(url, data);
  if (cache.size > MAX_CACHE_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
}

/** Drop all cached and in-flight image fetches (e.g. on logout). */
export function clearImageCache(): void {
  cache.clear();
  pending.clear();
}

interface ImageState {
  url: string | null;
  dataUrl: string | null;
}

interface RemoteImageHook {
  src: string | null;
  onError: () => void;
}

/**
 * Load a remote image (avatar/thumbnail) directly in the webview; if the load
 * fails (IG CDN rejects the request), fall back to fetching through the Rust
 * client and rendering the resulting data URL.
 */
export function useRemoteImage(url: string | null | undefined): RemoteImageHook {
  const [state, setState] = useState<ImageState>({ url: url ?? null, dataUrl: null });
  // Latched per URL rather than as a boolean: virtualized rows and the lightbox
  // reuse one hook instance across URLs, so a latch left set by the previous URL
  // would suppress the backend fallback for every later one.
  const failedRef = useRef<string | null>(null);
  if (state.url !== (url ?? null)) {
    setState({ url: url ?? null, dataUrl: null });
  }

  const onError = useCallback(() => {
    if (failedRef.current === url || !url) return;
    failedRef.current = url;
    const cached = cacheGet(url);
    if (cached) {
      setState({ url, dataUrl: cached });
      return;
    }
    // Concurrent misses for the same URL share one backend fetch.
    const fetchPromise = pending.get(url) ?? invoke<string | null>("fetch_image", { url });
    pending.set(url, fetchPromise);
    fetchPromise
      .then((d) => {
        if (d) {
          cacheSet(url, d);
          setState({ url, dataUrl: d });
        }
      })
      .catch((err) => {
        console.error("fetch_image failed:", url, err);
      })
      .finally(() => {
        pending.delete(url);
      });
  }, [url]);

  const src = url ? (state.dataUrl ?? url) : null;
  return { src, onError };
}

/** Deterministic avatar color from a name (theme.rs `hash_color`). */
export function hashColor(name: string): string {
  const palette = ["#7b3ff2", "#d53088", "#e6683c", "#0095f6", "#17a35d", "#cc2366"];
  let sum = 0;
  for (const ch of name) sum += ch.codePointAt(0) ?? 0;
  return palette[sum % palette.length];
}
