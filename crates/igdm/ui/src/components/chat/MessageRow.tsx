import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useReducedMotion } from "motion/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import Linkify from "linkify-react";
import type { Attachment } from "../../lib/attachment";
import type { MsgRow } from "../../lib/buildRows";
import Avatar from "../common/Avatar";
import { useRemoteImage } from "../../hooks/useRemoteImage";
import SuperParticles from "./SuperParticles";
import VoicePlayer from "./VoicePlayer";

/** One participant shown in the mini-avatar seen receipt stack. */
export interface SeenAvatar {
  pk: string;
  name: string;
  url: string | null;
}

export interface MessageView {
  id: string;
  text: string;
  own: boolean;
  senderName: string;
  senderUrl: string | null;
  group: boolean;
  /** First message of a consecutive run from the same sender (shows nickname + avatar). */
  firstInGroup: boolean;
  /** Last message of a consecutive run from the same sender (bubble shape). */
  lastInGroup: boolean;
  reactions: string;
  /** Who reacted with what, for the hover tooltip. */
  reactionDetails: { emoji: string; names: string[] }[];
  mediaLabel: string | null;
  unsupported: string;
  replyBlock: [string, string] | null;
  attachment: Attachment | null;
  system: string | null;
  /** Call icon shown next to system rows for call events. */
  systemIcon: "video" | "audio" | null;
  replyTarget: string | null;
  /** Mini avatars under the latest own message other participants saw. */
  seenBy: SeenAvatar[];
  /** True when any reaction on this message is an animated super reaction. */
  hasSuperReaction: boolean;
  /** True when the text was edited after sending (shows the "Edited" label). */
  edited: boolean;
}

/** Super emoji to animate over a message; `nonce` retriggers on re-send. */
export interface SuperBurst {
  emoji: string;
  nonce: number;
}

interface Props {
  /** The row model. Passed whole (instead of only `view`) so this component's
   * props stay referentially stable and `memo` can bail out: the parent used
   * to hand down a fresh arrow function per callback on every render, which
   * made every visible row re-render on each scroll tick. */
  row: MsgRow;
  /** True right after a reply quote jumped to this message: runs the
   * one-shot filter flash that marks the row (and its bubble). */
  highlight: boolean;
  /** True right after the message was added: plays the send pop-in once. */
  fresh: boolean;
  /** True while a confirmed delete waits for the backend: fade/collapse out. */
  removing: boolean;
  /** Super reaction animation to play over this bubble. */
  burst: SuperBurst | null;
  onMedia: (row: MsgRow) => void;
  onMenu: (x: number, y: number, row: MsgRow) => void;
  onReplyClick: (row: MsgRow) => void;
  onDoubleClick: (row: MsgRow) => void;
}

/** Padding around a bubble based on its group position. */
function rowPadFor(firstInGroup: boolean, lastInGroup: boolean): string {
  return firstInGroup
    ? lastInGroup
      ? "py-1"
      : "pt-1 pb-[2px]"
    : lastInGroup
      ? "pt-[2px] pb-1"
      : "py-[2px]";
}

/** Bubble corner rounding based on group position and sender side. */
function bubbleRadiusFor(own: boolean, firstInGroup: boolean, lastInGroup: boolean): string {
  if (own) {
    if (lastInGroup) {
      return firstInGroup
        ? "rounded-[18px]"
        : "rounded-tl-[18px] rounded-bl-[18px] rounded-br-[18px]";
    }
    return firstInGroup
      ? "rounded-tl-[18px] rounded-tr-[18px] rounded-bl-[18px]"
      : "rounded-tl-[18px] rounded-bl-[18px]";
  }
  if (lastInGroup) {
    return firstInGroup
      ? "rounded-[18px]"
      : "rounded-tr-[18px] rounded-br-[18px] rounded-bl-[18px]";
  }
  return firstInGroup
    ? "rounded-tl-[18px] rounded-tr-[18px] rounded-br-[18px]"
    : "rounded-tr-[18px] rounded-br-[18px]";
}

function AvatarCell({ view }: { view: MessageView }) {
  if (view.own || !view.firstInGroup) return <div className="w-7 shrink-0" />;
  return (
    <div className="shrink-0 self-start pt-0.5">
      <Avatar name={view.senderName} url={view.senderUrl} size={28} />
    </div>
  );
}

function NameTag({ view }: { view: MessageView }) {
  if (!view.group || view.own || !view.firstInGroup || view.senderName.length === 0) return null;
  return (
    <span className="pt-1.5 text-[11px]" style={{ color: "var(--ct-secondary)" }}>
      {view.senderName}
    </span>
  );
}

function ReplyBubble({
  block,
  own,
  onClick,
}: {
  block: [string, string];
  own: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="flex flex-col gap-0.5 rounded-lg border border-border-soft px-1.5 py-2 text-left"
      style={{ backgroundColor: "var(--ct-quote-in)" }}
      onClick={onClick}
      aria-label={`Reply to: ${block[0]}`}
    >
      <span
        className="text-[11px] font-semibold"
        style={{
          // Quote text renders on `--ct-quote-in`; `--ct-quote-text` is the
          // theme's contrast-safe quote color (falls back per side otherwise).
          color: `var(--ct-quote-text, ${own ? "var(--ct-text-out)" : "var(--ct-emphasis, var(--color-accent))"})`,
        }}
      >
        {block[0]}
      </span>
      <span
        className="line-clamp-2 text-[12px]"
        style={{
          color: `var(--ct-quote-text, ${own ? "var(--ct-text-out)" : "var(--ct-secondary)"})`,
          opacity: own ? 0.6 : 1,
        }}
      >
        {block[1]}
      </span>
    </button>
  );
}

function MediaContent({ view, onMedia }: { view: MessageView; onMedia: () => void }) {
  if (view.attachment) {
    return <AttachmentBody att={view.attachment} onMedia={onMedia} own={view.own} />;
  }
  if (view.mediaLabel) {
    return (
      <button
        type="button"
        className={`text-left text-[13.5px] font-semibold ${view.own ? "text-[#dcecff]" : "text-[#6cb8f0]"}`}
        onClick={onMedia}
        aria-label={`Open ${view.mediaLabel}`}
      >
        {view.mediaLabel}
      </button>
    );
  }
  return null;
}

function TextBody({ view, hasContent }: { view: MessageView; hasContent: boolean }) {
  if (view.text.length > 0) {
    return (
      <span className={view.edited ? "msg-text-edit" : undefined}>
        <LinkedText text={view.text} own={view.own} />
      </span>
    );
  }
  if (hasContent) return null;
  return (
    <span
      className="text-[13.5px] italic"
      style={{
        color: view.own ? "var(--ct-text-out)" : "var(--ct-secondary)",
        opacity: view.own ? 0.7 : 1,
      }}
    >
      {view.unsupported}
    </span>
  );
}

function Bubble({
  view,
  onMedia,
  onReplyClick,
}: {
  view: MessageView;
  onMedia: () => void;
  onReplyClick: () => void;
}) {
  const att = view.attachment;
  const shareLike =
    att !== null &&
    att.kind !== "media" &&
    att.kind !== "gif" &&
    att.kind !== "voice" &&
    att.kind !== "note";
  const hasContent = att !== null || view.mediaLabel !== null;
  return (
    <div
      className={`flex max-w-[420px] flex-col gap-0.5 px-3.5 py-2 ${bubbleRadiusFor(view.own, view.firstInGroup, view.lastInGroup)}`}
      style={{
        // Outgoing share cards sit on the flat incoming color — no
        // gradient behind content cards. Voice keeps the gradient.
        background: view.own && !shareLike ? "var(--ct-bubble-out)" : "var(--ct-bubble-in)",
      }}
    >
      {view.replyBlock ? (
        <ReplyBubble block={view.replyBlock} own={view.own} onClick={onReplyClick} />
      ) : null}
      <MediaContent view={view} onMedia={onMedia} />
      <TextBody key={view.text} view={view} hasContent={hasContent} />
    </div>
  );
}

/** Mini-avatar stack under the last own message other participants have seen. */
function SeenReceipts({ seen }: { seen: SeenAvatar[] }) {
  if (seen.length === 0) return null;
  // IG caps the receipt heads at 10 and rolls the rest into "+N".
  const shown = seen.slice(0, 10);
  const extra = seen.length - shown.length;
  const names = seen.map((s) => s.name).join(", ");
  return (
    <div className="mt-0.5 flex items-center pr-0.5" role="img" aria-label={`Seen by ${names}`}>
      {shown.map((avatar, i) => (
        <span
          key={avatar.pk}
          className="seen-avatar"
          style={{
            marginLeft: i === 0 ? 0 : -4,
            zIndex: shown.length - i,
            animationDelay: `${i * 40}ms`,
          }}
        >
          <Avatar name={avatar.name} url={avatar.url} size={15} />
        </span>
      ))}
      {extra > 0 && (
        <span className="ml-1 text-[9px] text-ink3" aria-hidden="true">
          +{extra}
        </span>
      )}
    </div>
  );
}

function ReactionsOverlay({ view }: { view: MessageView }) {
  // Pop the pill when its content changes; scroll-driven remounts must not
  // replay the animation, so the previous value is the trigger.
  const [popping, setPopping] = useState(false);
  const prevReactions = useRef(view.reactions);
  useEffect(() => {
    if (prevReactions.current === view.reactions) return;
    prevReactions.current = view.reactions;
    if (view.reactions.length === 0) return;
    setPopping(true);
    const t = window.setTimeout(() => setPopping(false), 320);
    return () => window.clearTimeout(t);
  }, [view.reactions]);
  if (view.reactions.length === 0) return null;
  return (
    <div className={`relative group ${view.own ? "self-end" : "self-start"}`}>
      <div
        className={`emoji-font select-none rounded-[9px] border px-2 py-0.5 text-[12px] ${
          view.hasSuperReaction ? "reaction-super" : ""
        } ${popping ? "reaction-pop" : ""}`}
        style={{
          backgroundColor: "var(--ct-reaction)",
          color: "var(--ct-text-in)",
          borderColor: "var(--ct-separator)",
        }}
      >
        {view.reactions}
      </div>
      {view.reactionDetails.length > 0 && (
        <div
          className={`reaction-details pointer-events-none absolute bottom-full z-30 mb-1.5 hidden w-max max-w-[240px] rounded-lg border border-border bg-elevated px-2.5 py-1.5 shadow-2xl group-hover:block ${
            view.own ? "right-0" : "left-0"
          }`}
        >
          {view.reactionDetails.map((d) => {
            const shown = d.names.slice(0, 5);
            const extra = d.names.length - shown.length;
            const key = `${d.emoji}-${view.id}`;
            return (
              <div key={key} className="flex items-baseline gap-2 py-0.5">
                <span className="emoji-font shrink-0 text-[13px]">{d.emoji}</span>
                <span className="text-[12px] text-ink2">
                  {shown.join(", ")}
                  {extra > 0 ? ` +${extra} more` : ""}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default memo(function MessageRow({
  row,
  highlight,
  fresh,
  removing,
  burst,
  onMedia,
  onMenu,
  onReplyClick,
  onDoubleClick,
}: Props) {
  const view = row.view;
  const reduce = useReducedMotion();
  // Subtle bump when the bubble text changes: an optimistic edit save and the
  // server confirmation both replace the message, so the edit lands twice at
  // most but the bump only replays when the string actually changes.
  const [bump, setBump] = useState(false);
  const prevText = useRef(view.text);
  useEffect(() => {
    if (prevText.current === view.text) return;
    prevText.current = view.text;
    setBump(true);
    const t = window.setTimeout(() => setBump(false), 280);
    return () => window.clearTimeout(t);
  }, [view.text]);
  // A super reaction over the bubble bumps it once (1 -> 1.03 -> 1), matching
  // the burst animation the parent just started.
  useEffect(() => {
    if (!burst) return;
    setBump(true);
    const t = window.setTimeout(() => setBump(false), 280);
    return () => window.clearTimeout(t);
  }, [burst?.nonce]);
  // Double-tap heart burst: purely decorative and local; the ❤️ reaction is
  // sent by the parent's `onDoubleClick`.
  const [hearts, setHearts] = useState<{ id: number; x: number; y: number }[]>([]);
  const heartSeq = useRef(0);
  const heartTimers = useRef<number[]>([]);
  useEffect(
    () => () => {
      for (const t of heartTimers.current) window.clearTimeout(t);
    },
    [],
  );
  const addHeart = (x: number, y: number) => {
    if (reduce) return;
    const id = ++heartSeq.current;
    setHearts((prev) => [...prev, { id, x, y }]);
    heartTimers.current.push(
      window.setTimeout(() => setHearts((prev) => prev.filter((h) => h.id !== id)), 750),
    );
  };
  // The parent keeps these stable per row, so `memo` holds: a row only
  // re-renders when its own message changes, not when the list re-renders.
  const handleMedia = () => onMedia(row);
  const handleReplyClick = () => onReplyClick(row);
  return (
    <div
      className={`group relative flex w-full items-start gap-2 px-3 ${rowPadFor(view.firstInGroup, view.lastInGroup)} ${
        highlight ? "reply-flash" : ""
      } ${fresh ? "send-pop" : ""} ${removing ? "msg-removing" : ""}`}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu(e.clientX, e.clientY, row);
      }}
      onDoubleClick={(e) => {
        // SAFETY: WebKit reports text nodes as event targets, so resolve the
        // containing element instead of rejecting non-elements.
        const target =
          e.target instanceof Element
            ? e.target
            : ((e.target as Node | null)?.parentElement ?? null);
        if (!target) return;
        // Links and buttons have their own single-click actions.
        if (target.closest("a, button")) return;
        const rect = e.currentTarget.getBoundingClientRect();
        addHeart(e.clientX - rect.left, e.clientY - rect.top);
        onDoubleClick(row);
      }}
    >
      <AvatarCell view={view} />
      <div className={`flex min-w-0 flex-1 flex-col ${view.own ? "items-end" : "items-start"}`}>
        <NameTag view={view} />
        <div className={`relative ${bump ? "bubble-bump" : ""}`}>
          <Bubble view={view} onMedia={handleMedia} onReplyClick={handleReplyClick} />
          {burst && !reduce && <SuperParticles key={burst.nonce} burst={burst} />}
        </div>
        {view.edited && (
          <span
            className="mt-0.5 pr-0.5 text-[10px]"
            style={{ color: "var(--ct-secondary)" }}
            title={`Edited ${new Date(row.msg.timestamp).toLocaleString()}`}
          >
            Edited
          </span>
        )}
        <ReactionsOverlay view={view} />
        <SeenReceipts seen={view.seenBy} />
      </div>
      {!view.own && <div className="w-4 shrink-0" />}
      {hearts.map((heart) => (
        <span
          key={heart.id}
          className="heart-burst emoji-font"
          style={{ left: heart.x, top: heart.y }}
          aria-hidden="true"
        >
          ❤️
        </span>
      ))}
      {/* Keyboard path to the message menu (edit/delete/react), revealed on
          hover or keyboard focus. */}
      <button
        type="button"
        aria-label="Message options"
        aria-haspopup="menu"
        className="absolute top-0 right-0.5 z-10 rounded-md px-1.5 py-0.5 text-[13px] leading-none text-ink3 opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:bg-panel2 focus-visible:opacity-100"
        onClick={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          onMenu(rect.left, rect.bottom + 2, row);
        }}
      >
        ⋯
      </button>
    </div>
  );
});

function LinkedText({ text, own }: { text: string; own: boolean }) {
  const linkifiedContent = useMemo(
    () => (
      <Linkify
        options={{
          render: ({ attributes, content }) => {
            const handleActivate = (e: React.MouseEvent | React.KeyboardEvent) => {
              e.preventDefault();
              e.stopPropagation();
              void openUrl(String(attributes.href ?? ""));
            };
            return (
              <button
                type="button"
                className="link-text cursor-pointer"
                onClick={handleActivate}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    handleActivate(e);
                  }
                }}
              >
                {content}
              </button>
            );
          },
        }}
      >
        {text}
      </Linkify>
    ),
    [text],
  );

  return (
    <span
      className="whitespace-pre-wrap text-[13.5px] leading-snug"
      style={{ color: own ? "var(--ct-text-out)" : "var(--ct-text-in)" }}
    >
      {linkifiedContent}
    </span>
  );
}

function RemoteImage({ url, className }: { url: string; className?: string }) {
  const { src, onError } = useRemoteImage(url);
  return (
    <img src={src ?? undefined} alt="" draggable={false} className={className} onError={onError} />
  );
}

type MediaAttachment = Extract<Attachment, { kind: "media" }>;
type GifAttachment = Extract<Attachment, { kind: "gif" }>;
type ShareCard = Extract<Attachment, { kind: "post" | "clip" | "story" }>;
type NoteAttachment = Extract<Attachment, { kind: "note" }>;

function MediaAttachmentBody({ att, onMedia }: { att: MediaAttachment; onMedia: () => void }) {
  // Fit the media's real aspect ratio into a bounded box (never upscale,
  // never crop). Falls back to square when dimensions are unknown.
  const MAX_W = 260;
  const MAX_H = 320;
  const scale = Math.min(MAX_W / att.width, MAX_H / att.height, 1);
  const w = Math.max(1, Math.round(att.width * scale));
  const h = Math.max(1, Math.round(att.height * scale));
  return (
    <button
      type="button"
      className="relative block overflow-hidden rounded-[10px]"
      style={{ width: w, height: h }}
      onClick={onMedia}
      aria-label="View media"
    >
      <RemoteImage url={att.thumbUrl} className="h-full w-full object-cover" />
      {att.videoUrl && (
        <span className="absolute right-2 bottom-2 flex h-[30px] w-[30px] items-center justify-center rounded-full bg-black/60 text-[13px] text-white">
          ▶
        </span>
      )}
    </button>
  );
}

function GifAttachmentBody({ att, onMedia }: { att: GifAttachment; onMedia: () => void }) {
  return (
    <button
      type="button"
      className="relative block overflow-hidden rounded-[10px]"
      onClick={onMedia}
      aria-label="View GIF"
    >
      <video
        src={att.videoUrl}
        autoPlay
        loop
        muted
        playsInline
        disablePictureInPicture
        className="block object-cover"
        style={{ width: att.width, height: att.height }}
      />
    </button>
  );
}

function ShareCardBody({ att, onMedia }: { att: ShareCard; onMedia: () => void }) {
  const isClip = att.kind === "clip";
  const isStory = att.kind === "story";
  return (
    <button className="flex w-[300px] flex-col gap-1 text-left" onClick={onMedia}>
      <span className="flex items-center gap-2">
        <Avatar name={att.authorName} url={att.authorIcon || null} size={32} />
        <span className="max-w-full truncate text-[13px] font-semibold text-ink">
          {att.authorName}
        </span>
      </span>
      <span className="relative h-[300px] w-[300px] overflow-hidden rounded-[10px]">
        {att.imageUrl.length > 0 ? (
          <RemoteImage url={att.imageUrl} className="h-full w-full object-cover" />
        ) : null}
        {(isClip || isStory) && (
          <span className="absolute top-2 right-2 flex h-[30px] w-[30px] items-center justify-center rounded-full bg-black/60 text-[13px] text-white">
            ▶
          </span>
        )}
      </span>
      <span className="line-clamp-3 text-[12px] text-ink2">
        {isClip && att.subtitle.length > 0 ? `${att.title} · ${att.subtitle}` : att.title}
      </span>
    </button>
  );
}

/** Reply-to-note card: the note text in a small quote bubble above the note
 * author's avatar circle. The user's reply text renders as the message bubble
 * below (see `TextBody`). */
function NoteBody({ att }: { att: NoteAttachment }) {
  return (
    <div className="max-w-[240px]">
      <div
        className="relative rounded-[14px] border border-border-soft px-2.5 py-1.5 text-[12.5px] leading-snug"
        style={{ backgroundColor: "var(--ct-quote-in)" }}
      >
        <span style={{ color: "var(--ct-quote-text, var(--ct-secondary))" }}>{att.text}</span>
        <span
          className="absolute -bottom-1 left-1/2 h-2 w-2 -translate-x-1/2 rotate-45 border-r border-b border-border-soft"
          style={{ backgroundColor: "var(--ct-quote-in)" }}
          aria-hidden="true"
        />
      </div>
      <div className="mt-2 flex justify-center">
        {att.authorIcon ? (
          <RemoteImage
            url={att.authorIcon}
            className="h-9 w-9 rounded-full object-cover ring-1 ring-border-soft"
          />
        ) : (
          <span className="h-9 w-9 rounded-full bg-panel2 ring-1 ring-border-soft" />
        )}
      </div>
    </div>
  );
}

function AttachmentBody({
  att,
  onMedia,
  own: viewOwn,
}: {
  att: Attachment;
  onMedia: () => void;
  own: boolean;
}) {
  switch (att.kind) {
    case "placeholder":
      return (
        <div className="rounded-lg border border-border-soft bg-white/5 px-1.5 py-2">
          <span className="text-[12.5px] italic text-ink2">{att.text}</span>
        </div>
      );
    case "media":
      return <MediaAttachmentBody att={att} onMedia={onMedia} />;
    case "gif":
      return <GifAttachmentBody att={att} onMedia={onMedia} />;
    case "voice":
      return <VoicePlayer att={att} own={viewOwn} />;
    case "post":
    case "clip":
    case "story":
      return <ShareCardBody att={att} onMedia={onMedia} />;
    case "sticker":
      return (
        <button className="block w-[220px]" onClick={onMedia}>
          <RemoteImage url={att.imageUrl} className="h-auto w-full" />
        </button>
      );
    case "note":
      return <NoteBody att={att} />;
  }
}
