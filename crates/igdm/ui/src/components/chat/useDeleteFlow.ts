// Delete flow: bottom-sheet state plus the row fade. The actual removal is
// confirmed by the backend `MessageDeleted` event, so the sheet stays open with
// a spinner until the row disappears (or an error toast + timeout releases it).

import { useCallback, useEffect, useState } from "react";
import type { DirectMessage, ThreadState } from "../../state";
import { useApp } from "../../hooks/useApp";

/** How long a confirmed delete keeps the row faded while MessageDeleted lands. */
const REMOVING_TIMEOUT_MS = 4000;
/** Release a spinner that never got a MessageDeleted (SendFailed toast). */
const CONFIRM_TIMEOUT_MS = 8000;

interface DeleteState {
  threadKey: string;
  msg: DirectMessage;
  pending: boolean;
  removing: boolean;
}

export function useDeleteFlow(ts: ThreadState | undefined) {
  const { deleteMessage } = useApp();
  const [state, setState] = useState<DeleteState | null>(null);
  const threadKey = ts?.key ?? null;

  // The row is gone (MessageDeleted applied) or another thread is open: the
  // sheet is hidden without an effect. A stale state object is replaced the
  // next time a delete starts, so nothing needs clearing on unmount.
  const visible =
    state !== null &&
    threadKey === state.threadKey &&
    ts !== undefined &&
    ts.messages.some((m) => m.id === state.msg.id)
      ? state
      : null;
  const pending = visible?.pending ?? false;
  const removingId = visible?.removing ? visible.msg.id : null;

  // If the backend never confirms, release the sheet (SendFailed toast).
  useEffect(() => {
    if (!pending) return;
    const timer = window.setTimeout(() => {
      setState((prev) => (prev ? { ...prev, pending: false, removing: false } : prev));
    }, CONFIRM_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [pending]);

  // A failed delete must not hide the row forever: un-fade after the timeout.
  useEffect(() => {
    if (removingId === null) return;
    const timer = window.setTimeout(() => {
      setState((prev) => (prev ? { ...prev, removing: false } : prev));
    }, REMOVING_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [removingId]);

  const open = useCallback(
    (msg: DirectMessage) => {
      if (!threadKey) return;
      setState({ threadKey, msg, pending: false, removing: false });
    },
    [threadKey],
  );

  const close = useCallback(() => setState(null), []);

  const confirm = useCallback(
    (forSelf: boolean) => {
      if (!state) return;
      deleteMessage(state.threadKey, state.msg.id, state.msg.client_context ?? null, forSelf);
      setState((prev) => (prev ? { ...prev, pending: true, removing: true } : prev));
    },
    [state, deleteMessage],
  );

  return { target: visible?.msg ?? null, pending, removingId, open, close, confirm };
}
