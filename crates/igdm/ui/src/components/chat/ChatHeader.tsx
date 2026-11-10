import { useEffect, useMemo, useState } from "react";
import { useApp } from "../../hooks/useApp";
import {
  avatarForThread,
  displayName,
  threadTitle,
  tsMillis,
  type ThreadState,
  type UserShort,
} from "../../state";
import Avatar from "../common/Avatar";
import { activeParticipants } from "../common/presence";

type SubtitleTone = "typing" | "live" | "muted";

interface Subtitle {
  text: string;
  tone: SubtitleTone;
}

/** Header subtitle precedence: typing > active now > seen > member list. */
function subtitleFor(
  ts: ThreadState,
  typers: string[],
  active: UserShort[],
  meId: string,
): Subtitle | null {
  if (typers.length === 1) {
    const name = ts.users.find((u) => u.pk === typers[0]);
    return {
      text: `${ts.nicknames[typers[0]] || name?.full_name || name?.username || "Someone"} typing…`,
      tone: "typing",
    };
  }
  if (typers.length > 1) {
    return { text: `${typers.length} people typing…`, tone: "typing" };
  }
  if (active.length > 0) {
    if (!ts.is_group) return { text: "Active now", tone: "live" };
    if (active.length <= 2) {
      return {
        text: `${active.map((u) => displayName(ts, u)).join(", ")} active`,
        tone: "live",
      };
    }
    return { text: `${active.length} active`, tone: "live" };
  }
  const last = ts.messages[ts.messages.length - 1];
  const ownLast = last && last.user_id === meId;
  const lastOwnTs = ownLast ? tsMillis(last) / 1000 : 0;
  const seenByOthers =
    ownLast &&
    Object.entries(ts.last_seen_at).some(([uid, t]) => uid !== meId && t > 0 && t >= lastOwnTs);
  if (seenByOthers) {
    return { text: "Seen", tone: "muted" };
  }
  if (ts.users.length > 0) {
    return {
      text: ts.users.map((u) => `@${u.username || "?"}`).join(", "),
      tone: "muted",
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

  const active = activeParticipants(state.presence, ts.users, state.me.user_id);
  const subtitle = subtitleFor(ts, typers, active, state.me.user_id);
  const avatar = avatarForThread(ts);
  const subtitleColor =
    subtitle?.tone === "typing"
      ? "var(--ct-emphasis, var(--color-accent))"
      : subtitle?.tone === "live"
        ? "var(--ig-live)"
        : "var(--ct-header-subtitle)";

  return (
    <div
      className="relative flex h-[60px] w-full shrink-0 items-center gap-3 border-b px-4"
      style={{
        backgroundColor: "var(--ct-header-bg)",
        borderColor: "var(--ct-separator)",
      }}
    >
      <Avatar
        name={avatar.name}
        url={avatar.url}
        size={44}
        presence={active.length > 0 ? "ring" : undefined}
      />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[14px] font-bold" style={{ color: "var(--ct-header-title)" }}>
          {threadTitle(ts)}
        </div>
        {subtitle && (
          <span
            key={`${subtitle.tone}:${subtitle.text}`}
            className="subtitle-crossfade block truncate text-[11px]"
            style={{ color: subtitleColor }}
          >
            {subtitle.text}
          </span>
        )}
      </div>
    </div>
  );
}
