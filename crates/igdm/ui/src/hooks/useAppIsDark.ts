import { useSyncExternalStore } from "react";
import { appIsDark } from "../lib/chatTheme";

/** Re-render when the app's color mode changes: the theme attribute the
 * Settings window sets, or the system preference while it is "system". */
function subscribe(onChange: () => void): () => void {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", onChange);
  return () => {
    observer.disconnect();
    media.removeEventListener("change", onChange);
  };
}

/** Whether the app is currently rendering in dark mode. */
export function useAppIsDark(): boolean {
  return useSyncExternalStore(subscribe, appIsDark);
}
