import type { DirectMessage } from "../state";
import { isString, type JsonObject } from "./guards";

const TIME_FMT = new Intl.DateTimeFormat("en", {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});
const SHORT_DATE_FMT = new Intl.DateTimeFormat("en", { month: "short", day: "numeric" });
const SHORT_DATE_YEAR_FMT = new Intl.DateTimeFormat("en", {
  month: "short",
  day: "numeric",
  year: "numeric",
});
const MONTH_DAY_FMT = new Intl.DateTimeFormat("en", { month: "long", day: "numeric" });
const LONG_DATE_FMT = new Intl.DateTimeFormat("en", {
  month: "long",
  day: "numeric",
  year: "numeric",
});

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function dayLabel(d: Date): string | null {
  const now = new Date();
  if (sameDay(d, now)) return "Today";
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameDay(d, yesterday)) return "Yesterday";
  return null;
}

/** Sidebar row time: HH:MM today, "Yesterday", else a short date. */
export function fmtTime(ts: number): string {
  if (ts <= 0) return "";
  const when = new Date(ts * 1000);
  const label = dayLabel(when);
  if (label) return label === "Today" ? TIME_FMT.format(when) : label;
  return when.getFullYear() === new Date().getFullYear()
    ? SHORT_DATE_FMT.format(when)
    : SHORT_DATE_YEAR_FMT.format(when);
}

/** Full date separator label. */
export function formatDate(d: Date): string {
  return (
    dayLabel(d) ??
    (d.getFullYear() === new Date().getFullYear()
      ? MONTH_DAY_FMT.format(d)
      : LONG_DATE_FMT.format(d))
  );
}

/** Short preview line for a DirectMessage. */
export function messagePreview(msg: DirectMessage): string {
  const t = msg.item_type ?? "";
  switch (t) {
    case "text":
      return msg.text ?? "";
    case "animated_media":
      return "GIF";
    case "media":
    case "visual_media":
    case "raven_media":
      return msg.media?.thumbnail_url ? "Photo" : "Media";
    case "voice_media":
      return "Voice message";
    case "media_share":
      return "Shared a post";
    case "xma_clip":
      return "Reel";
    case "xma_media_share":
      return "Post";
    case "xma_story_share":
      return "Story";
    case "xma_reel_mention":
      return "Reel mention";
    case "xma_share": {
      // SAFETY: raw is the xma payload; the keys below identify its type.
      const raw = msg.raw as JsonObject | null;
      if (raw) {
        if ("xma_clip" in raw) return "Reel";
        if ("xma_media_share" in raw) return "Post";
        if ("xma_story_share" in raw) return "Story";
      }
      return "Shared content";
    }
    case "action_log":
      return "";
    case "video_call_event":
    case "audio_call_event": {
      // SAFETY: raw is the call-event payload; the nested objects carry the text.
      const raw = msg.raw as JsonObject | null;
      // SAFETY: video_call_event carries a description string when present.
      const videoCall = raw?.video_call_event as JsonObject | null;
      // SAFETY: audio_call_event mirrors video_call_event's shape.
      const audioCall = raw?.audio_call_event as JsonObject | null;
      if (videoCall && isString(videoCall.description)) {
        return videoCall.description;
      }
      if (audioCall && isString(audioCall.description)) {
        return audioCall.description;
      }
      return t === "audio_call_event" ? "Audio call" : "Video call";
    }
    case "reel_share":
      return "Replied to a story";
    default: {
      if (msg.text && msg.text.length > 0) {
        return msg.text;
      }
      return t.length === 0 ? "<message>" : `<${t}>`;
    }
  }
}

/** Label for media-ish messages (rendered as the clickable media row). */
export function mediaLabel(msg: DirectMessage): string | null {
  const t = msg.item_type ?? "";
  if (t === "media" || t === "visual_media" || t === "raven_media") {
    const media = msg.media;
    if (media) {
      if (media.video_url) return "🎬 Video";
      if (media.thumbnail_url) return "📷 Photo";
      return "Media";
    }
  } else if (t === "voice_media") {
    const media = msg.media;
    if (media) return media.audio_url ? "🎤 Voice message" : "Voice message";
  } else if (t === "animated_media") {
    return "GIF";
  } else if (msg.media_share) {
    return "Shared post";
  } else if (msg.xma_share) {
    return "Shared content";
  }
  return null;
}

/** Reactions string for a message row. */
export function reactionsText(msg: DirectMessage): string {
  const reactions = msg.reactions;
  if (!reactions) return "";
  const emojiText = reactions.emojis.map((r) => emojiDisplay(r.emoji)).join("");
  const likes = reactions.likes_count;
  if (likes > 0 && emojiText.length === 0) {
    return String(likes);
  }
  return emojiText;
}

/** Strip VS16 from an emoji (canonical form used for comparisons). */
export function plainEmoji(emoji: string): string {
  return emoji.replace(/\u{fe0f}/gu, "");
}

export interface ReactionDetail {
  emoji: string;
  names: string[];
}

/** Group a message's reactions by emoji, resolving sender ids to names. */
export function reactionDetails(
  msg: DirectMessage,
  resolveName: (senderId: string) => string,
): ReactionDetail[] {
  const byEmoji = new Map<string, string[]>();
  for (const r of msg.reactions?.emojis ?? []) {
    const name = resolveName(String(r.sender_id));
    if (name.length === 0) continue;
    const list = byEmoji.get(r.emoji) ?? [];
    list.push(name);
    byEmoji.set(r.emoji, list);
  }
  return [...byEmoji.entries()].map(([emoji, names]) => ({ emoji, names }));
}

/** Force emoji (color) presentation by appending VS16 to codepoints that
 * default to text presentation (hearts, dingbats, misc symbols...). */
export function emojiDisplay(emoji: string): string {
  let out = "";
  for (const ch of emoji) {
    out += ch;
    const c = ch.codePointAt(0);
    if (c !== undefined && ((c >= 0x2600 && c <= 0x27bf) || (c >= 0x2b00 && c <= 0x2bff))) {
      out += "\u{fe0f}";
    }
  }
  return out;
}
