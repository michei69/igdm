import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/** Copy text to the clipboard. Returns false if the clipboard write failed
 * so callers can surface the failure instead of failing silently. */
export async function copyClipboard(text: string): Promise<boolean> {
  try {
    await writeText(text);
    return true;
  } catch {
    return false;
  }
}
