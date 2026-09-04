import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { applyTheme, isTheme, type Theme } from "../../lib/theme";

const EMOJI_SLOTS = [0, 1, 2, 3, 4];

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export default function SettingsScreen() {
  const [emojis, setEmojis] = useState<string[]>(["", "", "", "", ""]);
  const [saved, setSaved] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [theme, setTheme] = useState<Theme>("system");
  const [chatThemes, setChatThemes] = useState(true);

  useEffect(() => {
    api
      .getReactionEmojis()
      .then((list) => {
        if (list.length === 5) setEmojis(list);
      })
      .catch(() => {})
      .finally(() => setLoaded(true));
    api
      .getTheme()
      .then((t) => {
        if (isTheme(t)) {
          setTheme(t);
          applyTheme(t);
        }
      })
      .catch(() => {});
    api
      .getChatThemes()
      .then(setChatThemes)
      .catch(() => {});
  }, []);

  const canSave = loaded && emojis.every((e) => e.trim().length > 0);

  const doSave = () => {
    if (!canSave) return;
    api.saveReactionEmojis(emojis.map((e) => e.trim()));
    setSaved(true);
  };

  const doSetTheme = (t: Theme) => {
    setTheme(t);
    applyTheme(t);
    // The backend persists and broadcasts to every window.
    api.setTheme(t);
  };

  const doSetChatThemes = (v: boolean) => {
    setChatThemes(v);
    // The backend persists and broadcasts to every window.
    api.setChatThemes(v);
  };

  return (
    <div className="flex h-full w-full items-center justify-center bg-bg">
      <div className="flex w-[460px] flex-col gap-3 rounded-2xl border border-border bg-panel p-6">
        <div className="flex items-center justify-between">
          <h1 className="text-[20px] font-bold text-ink">Settings</h1>
        </div>
        <p className="text-[13px] text-ink2">Appearance:</p>
        <div className="flex gap-2">
          {THEMES.map((t) => (
            <button
              key={t.value}
              className={`flex-1 rounded-lg border px-3 py-2 text-[13px] transition-colors duration-150 ${
                theme === t.value
                  ? "border-accent bg-accent/10 font-semibold text-accent"
                  : "border-border bg-panel2 text-ink2 hover:bg-elevated"
              }`}
              onClick={() => doSetTheme(t.value)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className="flex items-center justify-between rounded-lg border border-border bg-panel2 px-3 py-2.5">
          <div>
            <div className="text-[13px] font-medium text-ink">Instagram chat themes</div>
            <div className="text-[11.5px] text-ink3">Per-chat bubble colors and background art</div>
          </div>
          <input
            type="checkbox"
            className="toggle toggle-sm"
            role="switch"
            aria-label="Instagram chat themes"
            checked={chatThemes}
            onChange={(e) => doSetChatThemes(e.target.checked)}
          />
        </div>
        <div className="mt-2 border-t border-border" />
        <p className="text-[13px] text-ink2">
          Reaction emojis shown in the message right-click menu:
        </p>
        <div className="flex gap-2">
          {EMOJI_SLOTS.map((slot) => (
            <input
              key={slot}
              className="emoji-font w-[72px] rounded-lg border border-border bg-panel2 px-2 py-1.5 text-center text-[20px] text-ink outline-none focus:border-accent"
              aria-label={`Reaction emoji ${slot + 1}`}
              value={emojis[slot]}
              maxLength={4}
              onChange={(ev) => {
                const next = [...emojis];
                next[slot] = ev.target.value;
                setEmojis(next);
                setSaved(false);
              }}
            />
          ))}
        </div>
        <button className="btn btn-primary w-full" disabled={!canSave} onClick={doSave}>
          Save
        </button>
        {saved && <p className="text-[13px] text-live">Saved ✓</p>}
      </div>
    </div>
  );
}
