// Focus plumbing for the composer: focus the input when a reply is chosen.
// Kept out of Composer so its render flow stays small.

import { useEffect, useRef, type RefObject } from "react";
import type { ReplyInfo } from "../../state";

/** Focus the input when a reply target is selected (not on thread switches
 * that keep an existing reply). */
export function useReplyFocus(
  reply: ReplyInfo | null,
  openKey: string,
  inputRef: RefObject<HTMLInputElement | null>,
): void {
  const prevReply = useRef<ReplyInfo | null>(null);
  useEffect(() => {
    const previous = prevReply.current;
    prevReply.current = reply;
    if (!reply || reply === previous) return;
    if (reply.threadKey !== openKey) return;
    inputRef.current?.focus();
  }, [reply, openKey, inputRef]);
}
