// Edit session shared by the message list (starts it) and the composer
// (prefills, saves and cancels it). Lives in the view layer so the data layer
// keeps only server state.

import { useCallback, useState } from "react";
import type { DirectMessage, ReplyInfo, ThreadState } from "../../state";
import type { EditTarget } from "./Composer";

export function useEditSession(
  openKey: string | null,
  threads: Record<string, ThreadState>,
  setReply: (reply: ReplyInfo | null) => void,
) {
  const [editing, setEditing] = useState<EditTarget | null>(null);

  const startEdit = useCallback(
    (msg: DirectMessage) => {
      if (!openKey) return;
      // An edit replaces the compose target; a pending reply quote would
      // otherwise be sent with the saved text.
      setReply(null);
      setEditing({
        threadKey: openKey,
        msgId: msg.id,
        text: msg.text ?? "",
        clientContext: msg.client_context ?? null,
      });
    },
    [openKey, setReply],
  );

  const cancelEdit = useCallback(() => setEditing(null), []);

  // Derive validity instead of resetting state in effects: switching threads
  // or deleting the edited message just hides the session.
  const editingActive =
    editing !== null &&
    editing.threadKey === openKey &&
    (threads[editing.threadKey]?.messages.some((m) => m.id === editing.msgId) ?? false)
      ? editing
      : null;

  return { editing: editingActive, startEdit, cancelEdit };
}
