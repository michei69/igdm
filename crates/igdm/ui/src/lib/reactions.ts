// Shared reaction logic: the toggle used by the context menu and the
// double-click shortcut, plus ingestion of incoming reaction items.

import type { DirectMessage, MessageReactions, ThreadState } from "../state";
import { plainEmoji } from "./format";
import { isNumber, isString, type JsonObject } from "./guards";

export interface ReactionToggle {
  reactions: MessageReactions;
  /** True when the viewer's existing reaction was removed. */
  del: boolean;
}

/** New reactions list when `viewerId` toggles `emoji` on `msg`. */
export function toggleReaction(
  msg: DirectMessage,
  emoji: string,
  viewerId: string,
): ReactionToggle {
  const plain = plainEmoji(emoji);
  const existing =
    msg.reactions?.emojis.filter(
      (r) => String(r.sender_id) === viewerId && plainEmoji(r.emoji) === plain,
    ) ?? [];
  const del = existing.length > 0;
  const base = msg.reactions ?? { likes: [], likes_count: 0, emojis: [] };
  const reactions = del
    ? {
        ...base,
        emojis: base.emojis.filter(
          (r) => !(String(r.sender_id) === viewerId && plainEmoji(r.emoji) === plain),
        ),
      }
    : {
        ...base,
        emojis: [
          ...base.emojis,
          {
            timestamp: new Date().toISOString(),
            client_context: null,
            sender_id: Number(viewerId) || 0,
            emoji,
            super_react_type: "none",
          },
        ],
      };
  return { reactions, del };
}

export interface IncomingReaction {
  messageId: string;
  emoji: string;
  superReactType: string;
  timestamp: string;
  senderId: string;
}

/**
 * Message rows are keyed by numeric `item_id`, but reaction payloads
 * reference their target by the canonical `mid.$...` id, which lives in the
 * raw payload's `id` field. Match either form.
 */
export function matchesMessage(m: DirectMessage, messageId: string): boolean {
  if (m.id === messageId) return true;
  // SAFETY: reaction payloads reference their target by the raw `id`; a
  // non-object raw yields an undefined `id`, never equal to a messageId.
  const raw = m.raw as JsonObject | null;
  if (raw && raw.id === messageId) return true;
  return false;
}

/** True when the payload is reaction-shaped (a like/unlike echo or a
 * `direct_reaction` item), even if it carries no actionable reaction. */
export function isReactionItem(msg: DirectMessage): boolean {
  // SAFETY: raw is the wire payload; `item_type` plus `emoji`/`message_id`
  // presence are the reaction discriminator.
  const raw = msg.raw as JsonObject | null;
  if (!raw) return false;
  return raw.item_type === "direct_reaction" || (isString(raw.emoji) && isString(raw.message_id));
}

/**
 * If `msg` is a reaction item rather than a real message, return the reaction
 * it carries. IG sends these as `direct_reaction` items (reaction list under
 * `reactions`) or as bare `{ emoji, message_id, ... }` payloads; both shapes
 * ride along in `msg.raw`.
 */
export function reactionFromItem(msg: DirectMessage): IncomingReaction | null {
  // SAFETY: raw is the wire payload; `item_type`, `emoji`, and `message_id`
  // are the reaction discriminator.
  const raw = msg.raw as JsonObject | null;
  if (!raw) return null;
  const bare = isString(raw.emoji) && isString(raw.message_id);
  if (raw.item_type !== "direct_reaction" && !bare) return null;
  const list = Array.isArray(raw.reactions) && raw.reactions.length > 0 ? raw.reactions : [raw];
  for (const entry of list) {
    // SAFETY: reaction list entries share the same payload shape.
    const r = entry as JsonObject | null;
    if (!r) continue;
    if (!isString(r.emoji) || !isString(r.message_id)) continue;
    const entrySender = isNumber(r.sender_id) || isString(r.sender_id) ? String(r.sender_id) : "";
    return {
      messageId: r.message_id,
      emoji: r.emoji,
      superReactType: isString(r.super_react_type) ? r.super_react_type : "none",
      timestamp: isNumber(r.timestamp) ? new Date(r.timestamp / 1000).toISOString() : msg.timestamp,
      senderId:
        isString(raw.user_id) && raw.user_id.length > 0
          ? raw.user_id
          : entrySender.length > 0
            ? entrySender
            : (msg.user_id ?? ""),
    };
  }
  return null;
}

/** Attach an incoming reaction to its target message (dedup by sender+emoji). */
export function applyReaction(
  messages: DirectMessage[],
  reaction: IncomingReaction,
): DirectMessage[] {
  return messages.map((m) => {
    if (!matchesMessage(m, reaction.messageId)) return m;
    const base = m.reactions ?? { likes: [], likes_count: 0, emojis: [] };
    const dup = base.emojis.some(
      (r) =>
        String(r.sender_id) === reaction.senderId &&
        plainEmoji(r.emoji) === plainEmoji(reaction.emoji),
    );
    if (dup) return m;
    return {
      ...m,
      reactions: {
        ...base,
        emojis: [
          ...base.emojis,
          {
            timestamp: reaction.timestamp,
            client_context: null,
            sender_id: Number(reaction.senderId) || 0,
            emoji: reaction.emoji,
            super_react_type: reaction.superReactType,
          },
        ],
      },
    };
  });
}

/** Remove an incoming reaction from its target message (un-react). */
export function removeReaction(
  messages: DirectMessage[],
  reaction: IncomingReaction,
): DirectMessage[] {
  return messages.map((m) => {
    if (!matchesMessage(m, reaction.messageId)) return m;
    const base = m.reactions;
    if (!base) return m;
    const emojis = base.emojis.filter(
      (r) =>
        !(
          String(r.sender_id) === reaction.senderId &&
          plainEmoji(r.emoji) === plainEmoji(reaction.emoji)
        ),
    );
    if (emojis.length === base.emojis.length) return m;
    return { ...m, reactions: { ...base, emojis } };
  });
}

/** Thread with the toggle reaction set applied to the matching message. */
export function threadWithReaction(
  thread: ThreadState,
  msgId: string,
  reactions: MessageReactions,
): ThreadState {
  return {
    ...thread,
    messages: thread.messages.map((m) => (m.id === msgId ? { ...m, reactions } : m)),
  };
}
