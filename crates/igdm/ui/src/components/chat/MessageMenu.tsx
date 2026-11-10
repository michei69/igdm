// Message context menu: reaction tray (normal + super mode), reply, edit,
// copy and delete. Extracted from MessageList to keep the list focused on
// rendering/virtualization.

import { forwardRef, useCallback, useImperativeHandle, useState } from "react";
import { toast } from "sonner";
import { useApp } from "../../hooks/useApp";
import type { DirectMessage } from "../../state";
import { copyClipboard } from "../../lib/clipboard";
import { senderOf } from "../../lib/buildRows";
import { emojiDisplay } from "../../lib/format";
import { SUPER_REACT_TYPE, threadWithReaction, toggleReaction } from "../../lib/reactions";
import ContextMenu, { MenuItem } from "../common/ContextMenu";
import { superReactionsFor } from "./superReact";

export interface MenuHostHandle {
  open: (x: number, y: number, msg: DirectMessage) => void;
}

interface MenuHostProps {
  onEdit: (msg: DirectMessage) => void;
  onDelete: (msg: DirectMessage) => void;
  onSuperReact: (msg: DirectMessage, emoji: string) => void;
}

export const MenuHost = forwardRef<MenuHostHandle, MenuHostProps>(function MenuHost(
  { onEdit, onDelete, onSuperReact },
  ref,
) {
  const [menu, setMenu] = useState<{ x: number; y: number; msg: DirectMessage } | null>(null);

  useImperativeHandle(
    ref,
    () => ({
      open: (x: number, y: number, msg: DirectMessage) => setMenu({ x, y, msg }),
    }),
    [],
  );

  if (!menu) return null;
  return (
    <MessageMenu
      menu={menu}
      onClose={() => setMenu(null)}
      onEdit={onEdit}
      onDelete={onDelete}
      onSuperReact={onSuperReact}
    />
  );
});

function MessageMenu({
  menu,
  onClose,
  onEdit,
  onDelete,
  onSuperReact,
}: {
  menu: { x: number; y: number; msg: DirectMessage };
  onClose: () => void;
  onEdit: (msg: DirectMessage) => void;
  onDelete: (msg: DirectMessage) => void;
  onSuperReact: (msg: DirectMessage, emoji: string) => void;
}) {
  const { state, updateThread, setReply, sendReaction } = useApp();
  const openKey = state.openKey;
  const ts = openKey ? state.threads[openKey] : undefined;
  const msg = menu.msg;
  const [superMode, setSuperMode] = useState(false);

  const react = useCallback(
    (emoji: string) => {
      if (!ts || openKey?.startsWith("user:")) {
        onClose();
        return;
      }
      const { reactions, del } = toggleReaction(msg, emoji, state.me.user_id);
      updateThread(ts.key, (t) => threadWithReaction(t, msg.id, reactions));
      sendReaction(ts.key, msg.id, emoji, del);
      onClose();
    },
    [ts, openKey, msg, state.me.user_id, updateThread, sendReaction, onClose],
  );

  const reactSuper = useCallback(
    (emoji: string) => {
      if (!ts || openKey?.startsWith("user:")) {
        onClose();
        return;
      }
      const reactions = superReactionsFor(msg, emoji, state.me.user_id);
      updateThread(ts.key, (t) => threadWithReaction(t, msg.id, reactions));
      onSuperReact(msg, emoji);
      sendReaction(ts.key, msg.id, emoji, false, SUPER_REACT_TYPE);
      onClose();
    },
    [ts, openKey, msg, state.me.user_id, updateThread, sendReaction, onSuperReact, onClose],
  );

  const doReply = useCallback(() => {
    if (!ts) return;
    const sender = senderOf(ts, msg, state.me.user_id);
    const text = (msg.text ?? "").slice(0, 60);
    setReply({
      threadKey: ts.key,
      msgId: msg.id,
      clientContext: msg.client_context ?? null,
      preview: sender.length > 0 ? `${sender}: ${text}` : text,
    });
    onClose();
  }, [ts, msg, state.me.user_id, setReply, onClose]);

  const doEdit = useCallback(() => {
    onEdit(msg);
    onClose();
  }, [msg, onEdit, onClose]);

  const doDelete = useCallback(() => {
    onDelete(msg);
    onClose();
  }, [msg, onDelete, onClose]);

  const doCopyText = useCallback(() => {
    if (msg.text) {
      void copyClipboard(msg.text);
      toast("Message text copied");
    }
    onClose();
  }, [msg.text, onClose]);

  const doCopyRaw = useCallback(() => {
    const raw = msg.raw ? JSON.stringify(msg.raw, null, 2) : JSON.stringify(msg, null, 2);
    void copyClipboard(raw);
    toast("Raw message data copied");
    onClose();
  }, [msg, onClose]);

  if (!ts) return null;

  // IG only lets the sender edit their own text; delete is offered for every
  // message (the sheet then narrows the options for incoming ones).
  const canEdit =
    msg.user_id === state.me.user_id &&
    (msg.text ?? "").trim().length > 0 &&
    msg.item_type !== "action_log";

  return (
    <ContextMenu x={menu.x} y={menu.y} onClose={onClose}>
      <div className="flex items-center justify-between gap-2 px-2 pt-1 pb-0.5">
        <span className="text-[11px] text-ink3">React</span>
        <button
          type="button"
          className={`rounded-full px-2 py-0.5 text-[11px] font-semibold transition-colors duration-150 ${
            superMode
              ? "bg-accent/20 text-accent"
              : "text-ink3 hover:bg-black/10 dark:hover:bg-white/10"
          }`}
          aria-pressed={superMode}
          aria-label="Super reaction mode"
          onClick={() => setSuperMode((on) => !on)}
        >
          ✨ Super
        </button>
      </div>
      <div className="flex items-center justify-center gap-0.5 px-2 py-1">
        {state.reactionEmojis.map((emoji) => (
          <button
            key={emoji}
            type="button"
            className={`emoji-font rounded-md p-1 text-ink transition-all duration-150 hover:bg-black/10 dark:hover:bg-white/10 ${
              superMode ? "text-[26px]" : "text-[18px]"
            }`}
            aria-label={superMode ? `Send super reaction ${emoji}` : `React with ${emoji}`}
            onClick={() => (superMode ? reactSuper(emoji) : react(emoji))}
          >
            {emojiDisplay(emoji)}
          </button>
        ))}
      </div>
      <MenuItem label="Reply" onSelect={doReply} />
      {canEdit && <MenuItem label="Edit" onSelect={doEdit} />}
      <MenuItem label="Copy text" onSelect={doCopyText} />
      <MenuItem label="Copy raw data" onSelect={doCopyRaw} />
      <MenuItem label="Delete" danger onSelect={doDelete} />
    </ContextMenu>
  );
}
