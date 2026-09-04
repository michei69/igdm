import { memo, useMemo } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import Linkify from "linkify-react";
import type { Attachment } from "../../lib/attachment";
import Avatar from "../common/Avatar";
import { useRemoteImage } from "../../hooks/useRemoteImage";
import VoicePlayer from "./VoicePlayer";

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
}

interface Props {
  view: MessageView;
  onMedia: () => void;
  onMenu: (x: number, y: number) => void;
  onReplyClick: () => void;
  onDoubleClick: () => void;
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
  if (view.text.length > 0) return <LinkedText text={view.text} own={view.own} />;
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
      <TextBody view={view} hasContent={hasContent} />
    </div>
  );
}

function ReactionsOverlay({ view }: { view: MessageView }) {
  if (view.reactions.length === 0) return null;
  return (
    <div className={`relative group ${view.own ? "self-end" : "self-start"}`}>
      <div
        className="emoji-font select-none rounded-[9px] border px-2 py-0.5 text-[12px]"
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
          className={`pointer-events-none absolute bottom-full z-30 mb-1.5 hidden w-max max-w-[240px] rounded-lg border border-border bg-elevated px-2.5 py-1.5 shadow-2xl group-hover:block ${
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
  view,
  onMedia,
  onMenu,
  onReplyClick,
  onDoubleClick,
}: Props) {
  return (
    <div
      className={`flex w-full items-start gap-2 px-3 ${rowPadFor(view.firstInGroup, view.lastInGroup)}`}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu(e.clientX, e.clientY);
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
        onDoubleClick();
      }}
    >
      <AvatarCell view={view} />
      <div className={`flex min-w-0 flex-1 flex-col ${view.own ? "items-end" : "items-start"}`}>
        <NameTag view={view} />
        <Bubble view={view} onMedia={onMedia} onReplyClick={onReplyClick} />
        <ReactionsOverlay view={view} />
      </div>
      {!view.own && <div className="w-4 shrink-0" />}
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
