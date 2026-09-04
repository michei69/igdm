import { useEffect, useMemo, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { avatarForThread, threadTitle, tsMillis, type ThreadState } from "../../state";
import Avatar from "../common/Avatar";

/** Header subtitle: typing indicator, seen receipt, or member list. */
function subtitleFor(
  ts: ThreadState,
  typers: string[],
  meId: string,
): { text: string; className: string } | null {
  if (typers.length === 1) {
    const name = ts.users.find((u) => u.pk === typers[0]);
    return {
      text: `${ts.nicknames[typers[0]] || name?.full_name || name?.username || "Someone"} typing…`,
      className: "emphasis",
    };
  }
  if (typers.length > 1) {
    return { text: `${typers.length} people typing…`, className: "emphasis" };
  }
  const last = ts.messages[ts.messages.length - 1];
  const ownLast = last && last.user_id === meId;
  const lastOwnTs = ownLast ? tsMillis(last) / 1000 : 0;
  const seenByOthers =
    ownLast &&
    Object.entries(ts.last_seen_at).some(([uid, t]) => uid !== meId && t > 0 && t >= lastOwnTs);
  if (seenByOthers) {
    return { text: "Seen", className: "text-ink3" };
  }
  if (ts.users.length > 0) {
    return {
      text: ts.users.map((u) => `@${u.username || "?"}`).join(", "),
      className: "text-ink3",
    };
  }
  return null;
}

export default function ChatHeader() {
  const { state } = useApp();
  const ts = state.openKey ? state.threads[state.openKey] : undefined;

  // Typing entries expire; tick while any are active so the indicator
  // clears on its own instead of lingering until the next re-render.
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const typingActive = !!ts && Object.values(ts.typing).some((expiry) => expiry > now);
  useEffect(() => {
    if (!typingActive) return;
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  }, [typingActive]);

  const typers = useMemo(() => {
    if (!ts) return [];
    const active: string[] = [];
    for (const [uid, expiry] of Object.entries(ts.typing)) {
      if (expiry > now) active.push(uid);
    }
    return active;
  }, [ts, now]);

  if (!ts) return null;

  const subtitle = subtitleFor(ts, typers, state.me.user_id);

  const avatar = avatarForThread(ts);

  return (
    <div
      className="relative flex h-[60px] w-full shrink-0 items-center gap-3 border-b px-4"
      style={{
        backgroundColor: "var(--ct-header-bg)",
        borderColor: "var(--ct-separator)",
      }}
    >
      <Avatar name={avatar.name} url={avatar.url} size={44} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[14px] font-bold" style={{ color: "var(--ct-header-title)" }}>
          {threadTitle(ts)}
        </div>
        {subtitle && (
          <div
            className="truncate text-[11px]"
            style={
              subtitle.className === "emphasis"
                ? { color: "var(--ct-emphasis, var(--color-accent))" }
                : { color: "var(--ct-header-subtitle)" }
            }
          >
            {subtitle.text}
          </div>
        )}
      </div>
    </div>
  );
}
