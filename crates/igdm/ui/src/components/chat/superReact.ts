// Shared super-reaction helpers: reaction classification, dedupe key, and the
// optimistic reaction list used while the server echo is in flight.

import type { DirectMessage, MessageReaction, MessageReactions } from "../../state";
import { plainEmoji } from "../../lib/format";
import { SUPER_REACT_TYPE } from "../../lib/reactions";

/** True when a reaction carries IG's animated ("super") reaction type. */
export function isSuperReaction(r: MessageReaction): boolean {
  return (r.super_react_type ?? "none") !== "none";
}

/** Dedupe key per reaction: sender+emoji is unique per message while it lives
 * (a user cannot hold two reactions with the same emoji on one message). */
export function superKeyFor(msgId: string, senderId: string, emoji: string): string {
  return `${msgId}:${senderId}:${plainEmoji(emoji)}`;
}

/** Optimistic reactions list for a super react. It upgrades an existing own
 * reaction instead of toggling it off: super react always sends. */
export function superReactionsFor(
  msg: DirectMessage,
  emoji: string,
  viewerId: string,
): MessageReactions {
  const base = msg.reactions ?? { likes: [], likes_count: 0, emojis: [] };
  const plain = plainEmoji(emoji);
  const mine = base.emojis.find(
    (r) => String(r.sender_id) === viewerId && plainEmoji(r.emoji) === plain,
  );
  if (mine) {
    return {
      ...base,
      emojis: base.emojis.map((r) =>
        r === mine ? { ...r, super_react_type: SUPER_REACT_TYPE } : r,
      ),
    };
  }
  return {
    ...base,
    emojis: [
      ...base.emojis,
      {
        timestamp: new Date().toISOString(),
        client_context: null,
        sender_id: Number(viewerId) || 0,
        emoji,
        super_react_type: SUPER_REACT_TYPE,
      },
    ],
  };
}
