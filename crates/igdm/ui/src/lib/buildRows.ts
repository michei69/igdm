// Pure row-model builder for the virtualized message list.

import type { DirectMessage, ThreadState, UserShort } from "../state";
import { displayName } from "../state";
import { attachmentFromMsg, type Attachment, type AuthorResolver } from "./attachment";
import { mediaLabel, messagePreview, reactionDetails, reactionsText } from "./format";
import { type Json, type JsonObject } from "./guards";
import type { MessageView } from "../components/chat/MessageRow";

export type MsgRow = {
  kind: "msg";
  key: string;
  view: MessageView;
  msg: DirectMessage;
  replyTarget: string | null;
};

export type Row =
  | { kind: "hint"; key: string; loading: boolean }
  | { kind: "day"; key: string; when: Date }
  | MsgRow;

/** Display-name resolution shared by `senderOf` and reaction names. */
function resolveName(
  ts: ThreadState,
  userId: string | null | undefined,
  viewerId: string,
  userMap: Map<string, UserShort>,
): string {
  if (userId === viewerId) return "You";
  const sender = userMap.get(userId ?? "");
  return sender ? displayName(ts, sender) : "";
}

export function senderOf(
  ts: ThreadState,
  msg: DirectMessage,
  viewerId: string,
  userMap?: Map<string, UserShort>,
): string {
  const map = userMap ?? new Map(ts.users.map((u) => [u.pk, u]));
  return resolveName(ts, msg.user_id, viewerId, map);
}

/** Row-view fields shared by system and message rows; the grouping pass at
 * the end overwrites the group flags. */
function baseView(
  msg: DirectMessage,
  view: Omit<MessageView, "id" | "firstInGroup" | "lastInGroup">,
): MessageView {
  return { id: msg.id, firstInGroup: false, lastInGroup: false, ...view };
}

function systemView(
  msg: DirectMessage,
  description: string,
  systemIcon: "video" | "audio" | null = null,
): MessageView {
  return baseView(msg, {
    text: "",
    own: false,
    senderName: "",
    senderUrl: null,
    group: false,
    reactions: "",
    reactionDetails: [],
    mediaLabel: null,
    unsupported: "",
    replyBlock: null,
    attachment: null,
    system: description,
    systemIcon,
    replyTarget: null,
  });
}

/** Call event payload: `video_call_event`/`audio_call_event` item types or
 * raw keys; audio-only calls arrive as video_call_event items with
 * `thread_has_audio_only_call: true`. The description is `messagePreview`'s
 * (same extraction as the sidebar preview). */
function callEventOf(msg: DirectMessage): { icon: "video" | "audio"; description: string } | null {
  // SAFETY: raw is the call-event payload; the video_call_event/audio_call_event
  // keys identify the call type and thread_has_audio_only_call marks audio.
  const raw = msg.raw as JsonObject | null;
  if (!raw) return null;
  const t = msg.item_type ?? "";
  let call: Json | null = null;
  let icon: "video" | "audio" = "video";
  if (t === "video_call_event" || "video_call_event" in raw) {
    if ("video_call_event" in raw) call = raw.video_call_event;
  } else if (t === "audio_call_event" || "audio_call_event" in raw) {
    if ("audio_call_event" in raw) call = raw.audio_call_event;
    icon = "audio";
  } else {
    return null;
  }
  // SAFETY: call is the call object under the matched key; the audio-only
  // flag is a boolean when present.
  const callObj = call as JsonObject | null;
  if (callObj && callObj.thread_has_audio_only_call === true) {
    icon = "audio";
  }
  return { icon, description: messagePreview(msg) };
}

/** Per-message row data, computed once per message identity. Messages are
 * immutable (the reducer replaces them on change), so typing/seen/reaction
 * events — which re-run the whole builder over the same message objects —
 * skip the per-message Date parses and share-payload JSON.parse entirely. */
interface MessageCache {
  dayKey: string;
  whenMs: number;
  attachment: Attachment | null;
  reactionsText: string;
}

const messageCache = new WeakMap<DirectMessage, MessageCache>();

function cacheFor(msg: DirectMessage, resolveAuthor: AuthorResolver): MessageCache {
  let cached = messageCache.get(msg);
  if (!cached) {
    const whenMs = new Date(msg.timestamp).getTime();
    cached = {
      dayKey: new Date(whenMs).toDateString(),
      whenMs,
      attachment: attachmentFromMsg(msg, resolveAuthor),
      reactionsText: reactionsText(msg),
    };
    messageCache.set(msg, cached);
  }
  return cached;
}

export function buildRows(ts: ThreadState, meId: string): Row[] {
  const items: Row[] = [];
  if (ts.has_more && ts.oldest_cursor) {
    items.push({ kind: "hint", key: "hint", loading: ts.loading_older });
  }
  const userMap = new Map(ts.users.map((u) => [u.pk, u]));
  const resolveAuthor: AuthorResolver = (uid) => {
    // Shares without an author (reel mentions) assume the sender.
    if (uid === meId) return { name: "You", url: null };
    const user = userMap.get(uid ?? "");
    return user ? { name: displayName(ts, user), url: user.profile_pic_url ?? null } : null;
  };
  // Days whose only rows are action logs or call events (or reaction logs,
  // which are skipped entirely) get no date separator — a lone marker with
  // nothing under it is noise.
  const realDays = new Set<string>();
  for (const m of ts.messages) {
    if (m.item_type !== "action_log" && callEventOf(m) === null) {
      realDays.add(cacheFor(m, resolveAuthor).dayKey);
    }
  }
  let prevDay = "";
  for (const msg of ts.messages) {
    const cached = cacheFor(msg, resolveAuthor);
    const day = cached.dayKey;
    if (day !== prevDay && realDays.has(day)) {
      items.push({ kind: "day", key: `sep:${day}`, when: new Date(cached.whenMs) });
      prevDay = day;
    }
    const call = callEventOf(msg);
    if (call) {
      items.push({
        kind: "msg",
        key: msg.id,
        view: systemView(msg, call.description, call.icon),
        msg,
        replyTarget: null,
      });
      continue;
    }
    if (msg.item_type === "action_log") {
      const log = msg.action_log;
      if (log?.is_reaction_log) continue;
      const desc = log?.description || log?.text_parts?.[0]?.text || null;
      if (desc) {
        items.push({
          kind: "msg",
          key: msg.id,
          view: systemView(msg, desc),
          msg,
          replyTarget: null,
        });
      }
      continue;
    }
    const own = msg.user_id === meId;
    const sender = userMap.get(msg.user_id ?? "");
    const senderName = sender ? displayName(ts, sender) : "";
    const senderUrl = sender?.profile_pic_url ?? null;
    // Unknown senders keep the "?" fallback (senderOf returns "").
    const nameFor = (id: string): string =>
      resolveName(ts, id, meId, userMap) || displayName(ts, userMap.get(id) ?? { pk: id });
    const replyBlock: [string, string] | null = msg.reply
      ? [
          senderOf(ts, msg.reply, meId, userMap) || "?",
          msg.reply.text && msg.reply.text.length > 0
            ? msg.reply.text
            : (mediaLabel(msg.reply) ?? "<message>"),
        ]
      : null;
    const replyTarget = msg.reply ? msg.reply.id : null;
    items.push({
      kind: "msg",
      key: msg.id,
      view: baseView(msg, {
        text: msg.text ?? "",
        own,
        senderName,
        senderUrl,
        group: ts.is_group,
        reactions: cached.reactionsText,
        reactionDetails: reactionDetails(msg, nameFor),
        mediaLabel: mediaLabel(msg),
        unsupported: `Unsupported message type (${msg.item_type ?? "unknown"})`,
        replyBlock,
        attachment: cached.attachment,
        system: null,
        systemIcon: null,
        replyTarget,
      }),
      msg,
      replyTarget,
    });
  }

  // Consecutive messages from the same sender form a "group": the first shows
  // the nickname and avatar, and the bubbles between share tighter spacing
  // and squared corners. System rows and date separators break the chain.
  const setGroup = (idx: number, first: boolean, last: boolean) => {
    const row = items[idx];
    if (row.kind !== "msg" || row.view.system !== null) return;
    items[idx] = { ...row, view: { ...row.view, firstInGroup: first, lastInGroup: last } };
  };
  let groupStart = -1;
  let prevSender: string | null = null;
  const closeGroup = (lastIdx: number) => {
    if (groupStart < 0) return;
    setGroup(groupStart, true, groupStart === lastIdx);
    for (let i = groupStart + 1; i <= lastIdx; i++) setGroup(i, false, i === lastIdx);
    groupStart = -1;
  };
  for (let i = 0; i < items.length; i++) {
    const row = items[i];
    const sender = row.kind === "msg" && row.view.system === null ? row.msg.user_id || null : null;
    if (sender === null) {
      closeGroup(i - 1);
      prevSender = null;
      groupStart = -1;
      continue;
    }
    // A reacted message stands alone: it never joins a run, and the run
    // splits around it.
    if (row.kind === "msg" && row.view.reactions.length > 0) {
      closeGroup(i - 1);
      setGroup(i, true, true);
      prevSender = null;
      groupStart = -1;
      continue;
    }
    if (sender === prevSender) continue;
    closeGroup(i - 1);
    prevSender = sender;
    groupStart = i;
  }
  closeGroup(items.length - 1);

  return items;
}
