import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { useApp } from "../../hooks/useApp";
import { useReplyHunt } from "../../hooks/useReplyHunt";
import type { DirectMessage, ThreadState } from "../../state";
import {
  attachmentFromMsg,
  attachmentOpenUrl,
  attachmentPreview,
  type MediaPreview,
  type ShareAttachment,
} from "../../lib/attachment";
import { emojiDisplay } from "../../lib/format";
import { copyClipboard } from "../../lib/clipboard";
import { senderOf, type Row } from "../../lib/buildRows";
import { threadWithReaction, toggleReaction } from "../../lib/reactions";
import MessageRow from "./MessageRow";
import DateSeparator from "../common/DateSeparator";
import ContextMenu, { MenuItem } from "../common/ContextMenu";
import { useMessageListVirtualization } from "./useMessageListVirtualization";

/** Small video/phone glyph for call-event system rows. */
function CallIcon({ kind }: { kind: "video" | "audio" }) {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="shrink-0"
      aria-hidden="true"
    >
      {kind === "video" ? (
        <>
          <rect x="2" y="6" width="13" height="12" rx="2" />
          <path d="m22 8-5 4 5 4V8z" />
        </>
      ) : (
        <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z" />
      )}
    </svg>
  );
}

interface Props {
  onPreview: (preview: MediaPreview) => void;
  onShare: (share: ShareAttachment) => void;
}

export default function MessageList({ onPreview, onShare }: Props) {
  const { state, updateThread, loadOlder, setReplyScroll, sendReaction } = useApp();
  const huntReply = useReplyHunt();
  const openKey = state.openKey;
  const ts: ThreadState | undefined = openKey ? state.threads[openKey] : undefined;
  const meId = state.me.user_id;

  const menuHostRef = useRef<{ open: (x: number, y: number, msg: DirectMessage) => void }>(null);

  const { scrollElRef, rows, virtualizer, totalSize, showJump, jumpToBottom, loadOlderIfNeeded } =
    useMessageListVirtualization({ ts, meId, openKey, loadOlder });

  // Scroll to the reply target once its row exists.
  useEffect(() => {
    const target = state.replyScroll;
    if (!target) return;
    const idx = rows.findIndex((r) => r.key === target);
    if (idx >= 0) {
      virtualizer.scrollToIndex(idx, { align: "start", behavior: "smooth" });
      setReplyScroll(null, 0);
    }
  }, [state.replyScroll, rows, virtualizer, setReplyScroll]);

  const onMedia = useCallback(
    (msg: DirectMessage) => {
      const att = attachmentFromMsg(msg);
      if (!att) return;
      // Reel, feed and story shares open the detail modal (download + open in
      // web); only older share shapes fall through to the browser.
      if (att.kind === "clip" || att.kind === "post" || att.kind === "story") {
        onShare(att);
        return;
      }
      const url = attachmentOpenUrl(att);
      if (url) {
        void openUrl(url);
        return;
      }
      const preview = attachmentPreview(att);
      if (preview) onPreview(preview);
    },
    [onPreview, onShare],
  );

  const handleReaction = useCallback(
    (msg: DirectMessage, emoji: string) => {
      if (!ts || openKey?.startsWith("user:")) return;
      const { reactions, del } = toggleReaction(msg, emoji, state.me.user_id);
      updateThread(ts.key, (t) => threadWithReaction(t, msg.id, reactions));
      sendReaction(ts.key, msg.id, emoji, del);
    },
    [ts, openKey, state.me.user_id, updateThread, sendReaction],
  );

  const onDoubleClickMsg = useCallback(
    (msg: DirectMessage) => {
      if (!ts || openKey?.startsWith("user:")) return;
      const emoji = state.reactionEmojis[0];
      if (!emoji) return;
      handleReaction(msg, emoji);
    },
    [ts, openKey, state.reactionEmojis, handleReaction],
  );

  const onReplyClick = useCallback(
    (row: Row) => {
      if (row.kind === "msg" && row.replyTarget && ts) {
        huntReply(ts, row.replyTarget);
      }
    },
    [ts, huntReply],
  );

  if (!ts) return null;

  return (
    <div className="relative h-full w-full">
      <div ref={scrollElRef} className="h-full w-full overflow-y-auto" data-testid="message-list">
        {ts.messages.length === 0 && !ts.loaded ? (
          <div className="flex flex-col gap-2 px-3 pt-4" aria-hidden="true">
            {Array.from({ length: 6 }, (_, i) => {
              const own = i % 3 === 1;
              return (
                <div key={i} className={`flex items-start gap-2 ${own ? "flex-row-reverse" : ""}`}>
                  {!own && <div className="skeleton h-7 w-7 shrink-0 rounded-full" />}
                  <div
                    className={`skeleton h-10 w-[55%] rounded-2xl ${own ? "rounded-tr-lg" : "rounded-tl-lg"}`}
                  />
                </div>
              );
            })}
          </div>
        ) : (
          <div style={{ height: totalSize, position: "relative" }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              const itemStyle: CSSProperties = {
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${item.start}px)`,
              };
              if (row.kind === "hint") {
                return (
                  <button
                    key={row.key}
                    type="button"
                    ref={virtualizer.measureElement}
                    data-index={item.index}
                    style={itemStyle}
                    className="flex h-8 w-full items-center justify-center"
                    onClick={loadOlderIfNeeded}
                  >
                    <span className="text-[11px] text-ink3">
                      {row.loading
                        ? "Loading earlier messages…"
                        : "Scroll up to load earlier messages"}
                    </span>
                  </button>
                );
              }
              if (row.kind === "day") {
                return (
                  <div
                    key={row.key}
                    ref={virtualizer.measureElement}
                    data-index={item.index}
                    style={itemStyle}
                    className="flex w-full justify-center"
                  >
                    <DateSeparator when={row.when} />
                  </div>
                );
              }
              const view = row.view;
              if (view.system !== null) {
                return (
                  <div
                    key={row.key}
                    ref={virtualizer.measureElement}
                    data-index={item.index}
                    style={itemStyle}
                    className="flex w-full justify-center px-4 pt-2.5 pb-1.5"
                  >
                    <span
                      className="flex max-w-full items-center gap-1.5 text-[11px]"
                      style={{ color: "var(--ct-secondary)" }}
                    >
                      {view.systemIcon && <CallIcon kind={view.systemIcon} />}
                      <span className="min-w-0 truncate">{view.system}</span>
                    </span>
                  </div>
                );
              }
              return (
                <div
                  key={row.key}
                  ref={virtualizer.measureElement}
                  data-index={item.index}
                  style={itemStyle}
                >
                  <MessageRow
                    view={view}
                    onMedia={() => onMedia(row.msg)}
                    onMenu={(x, y) => menuHostRef.current?.open(x, y, row.msg)}
                    onReplyClick={() => onReplyClick(row)}
                    onDoubleClick={() => onDoubleClickMsg(row.msg)}
                  />
                </div>
              );
            })}
          </div>
        )}
      </div>

      <MenuHost ref={menuHostRef} />

      {showJump && (
        <button
          className="btn btn-primary absolute right-5 bottom-5 z-10 h-10 w-10 rounded-full p-0 text-[18px]"
          onClick={jumpToBottom}
          aria-label="Jump to latest messages"
        >
          ↓
        </button>
      )}
    </div>
  );
}

const MenuHost = forwardRef<{ open: (x: number, y: number, msg: DirectMessage) => void }>(
  function MenuHost(_, ref) {
    const [menu, setMenu] = useState<{ x: number; y: number; msg: DirectMessage } | null>(null);

    useImperativeHandle(
      ref,
      () => ({
        open: (x: number, y: number, msg: DirectMessage) => setMenu({ x, y, msg }),
      }),
      [],
    );

    if (!menu) return null;
    return <MessageMenu menu={menu} onClose={() => setMenu(null)} />;
  },
);

function MessageMenu({
  menu,
  onClose,
}: {
  menu: { x: number; y: number; msg: DirectMessage };
  onClose: () => void;
}) {
  const { state, updateThread, setReply, sendReaction } = useApp();
  const openKey = state.openKey;
  const ts = openKey ? state.threads[openKey] : undefined;
  const msg = menu.msg;

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

  return (
    <ContextMenu x={menu.x} y={menu.y} onClose={onClose}>
      <div className="flex items-center justify-center gap-0.5 px-2 py-1.5">
        {state.reactionEmojis.map((emoji) => (
          <button
            key={emoji}
            className="emoji-font rounded-md p-1 text-[18px] text-ink transition-colors duration-150 hover:bg-black/10 dark:hover:bg-white/10"
            onClick={() => react(emoji)}
          >
            {emojiDisplay(emoji)}
          </button>
        ))}
      </div>
      <MenuItem label="Reply" onSelect={doReply} />
      <MenuItem label="Copy text" onSelect={doCopyText} />
      <MenuItem label="Copy raw data" onSelect={doCopyRaw} />
    </ContextMenu>
  );
}
