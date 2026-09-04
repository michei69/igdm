import type { DirectMessage, ThreadState } from "../state";
import { avatarForThread, threadTitle } from "../state";
import { fmtTime, messagePreview } from "./format";
import { isReactionItem } from "./reactions";

export interface ThreadView {
  key: string;
  title: string;
  preview: string;
  time: string;
  avatarUrl: string | null;
  avatarName: string;
  unread: boolean;
}

/** The last real message in a thread. Reaction echoes (likes) and reaction
 * log items sit at the end of the server's `items` array but are not
 * messages: they must not drive the sidebar preview or read receipts. */
export function lastMessageFrom(ts: ThreadState): DirectMessage | undefined {
  for (let i = ts.messages.length - 1; i >= 0; i--) {
    const m = ts.messages[i];
    if (isReactionItem(m)) continue;
    if (m.item_type === "action_log" && m.action_log?.is_reaction_log) continue;
    return m;
  }
  return undefined;
}

export function threadViewFromState(ts: ThreadState): ThreadView {
  const avatar = avatarForThread(ts);
  const last = lastMessageFrom(ts);
  const when =
    ts.last_activity > 0 ? ts.last_activity : last ? new Date(last.timestamp).getTime() / 1000 : 0;
  return {
    key: ts.key,
    title: threadTitle(ts),
    preview: last ? messagePreview(last) : "",
    time: fmtTime(when),
    avatarUrl: avatar.url,
    avatarName: avatar.name,
    unread: ts.unread,
  };
}
