import { useMemo, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import Lightbox, { type Slide } from "yet-another-react-lightbox";
import Download from "yet-another-react-lightbox/plugins/download";
import Video from "yet-another-react-lightbox/plugins/video";
import Zoom from "yet-another-react-lightbox/plugins/zoom";
import "yet-another-react-lightbox/styles.css";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useApp } from "../hooks/useApp";
import { useReplyHunt } from "../hooks/useReplyHunt";
import Sidebar from "./sidebar/Sidebar";
import ChatHeader from "./chat/ChatHeader";
import ApproveBar from "./chat/ApproveBar";
import MessageList from "./chat/MessageList";
import Composer from "./chat/Composer";
import ShareModal from "./chat/ShareModal";
import { useRemoteImage } from "../hooks/useRemoteImage";
import type { MediaPreview, ShareAttachment } from "../lib/attachment";
import { appIsDark, pickThreadTheme, themeBackgroundUrl, threadThemeVars } from "../lib/chatTheme";

export default function MainScreen() {
  const { state, setReplyScroll, setReply, downloadMedia } = useApp();
  const huntReply = useReplyHunt();
  const openKey = state.openKey;
  const hasThread = openKey !== null && !!state.threads[openKey];
  const empty = openKey === null || !hasThread;

  const [preview, setPreview] = useState<MediaPreview | null>(null);
  const [share, setShare] = useState<ShareAttachment | null>(null);

  const replyBar = state.reply && state.reply.threadKey === openKey ? state.reply : null;

  const onReplyBarClick = () => {
    if (!replyBar || !openKey) return;
    huntReply(state.threads[openKey], replyBar.msgId);
  };

  // Per-thread IG theme: pick the variant matching the app's color mode and
  // expose it to the message pane via CSS variables.
  const ts = openKey ? state.threads[openKey] : undefined;
  const themeData = useMemo(() => {
    if (!state.chatThemesEnabled || !ts?.theme_data) return null;
    const chatTheme = pickThreadTheme(ts.theme_data, appIsDark());
    if (!chatTheme) return null;
    return {
      vars: threadThemeVars(chatTheme),
      bgUrl: themeBackgroundUrl(chatTheme),
    };
  }, [ts?.theme_data, state.chatThemesEnabled]);
  const themeVars = themeData?.vars ?? undefined;
  const themeBgUrl = themeData?.bgUrl ?? null;

  return (
    <Group orientation="horizontal" className="h-full w-full bg-bg">
      <Panel
        defaultSize={300}
        minSize={220}
        maxSize={560}
        groupResizeBehavior="preserve-pixel-size"
        className="h-full"
      >
        <Sidebar />
      </Panel>

      <Separator className="relative w-2 shrink-0 cursor-col-resize text-border transition-colors [&[data-separator='hover']]:text-accent-hover [&[data-separator='active']]:text-accent">
        <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-current" />
      </Separator>

      {/* Right pane */}
      <Panel className="h-full min-w-0">
        <div
          className="chat-scope relative isolate flex h-full w-full min-w-0 flex-col"
          style={{ backgroundColor: "var(--ct-bg)", ...themeVars }}
        >
          {themeBgUrl && <ChatBackground url={themeBgUrl} />}
          <ChatHeader />
          <ApproveBar />
          <div className="relative h-0 min-h-0 flex-1">
            {empty ? (
              <div className="flex h-full w-full items-center justify-center">
                <p className="whitespace-pre-line text-center text-ink2">
                  {"Select a chat to start messaging\n—or search for someone new"}
                </p>
              </div>
            ) : (
              <MessageList onPreview={setPreview} onShare={setShare} />
            )}
          </div>

          {replyBar && openKey && (
            <div className="flex w-full items-center gap-2 border-t border-border bg-panel px-3.5 py-2.5">
              <button
                className="min-w-0 flex-1 truncate text-left text-[12px] text-ink2"
                onClick={onReplyBarClick}
              >
                ↩ {replyBar.preview}
              </button>
              <button
                className="shrink-0 text-[14px] text-ink3"
                onClick={() => {
                  setReplyScroll(null, 0);
                  setReply(null);
                }}
                aria-label="Cancel reply"
              >
                ✕
              </button>
            </div>
          )}

          <Composer />
        </div>
      </Panel>

      {/* Media preview lightbox */}
      {preview && (
        <MediaLightbox
          preview={preview}
          onClose={() => setPreview(null)}
          onDownload={downloadMedia}
        />
      )}

      {/* Reel/feed share modal: plays its own exit animation, then unmounts. */}
      {share && (
        <ShareModal
          share={share}
          onClose={() => setShare(null)}
          onDownload={downloadMedia}
          onOpenWeb={(url) => void openUrl(url)}
        />
      )}
    </Group>
  );
}

function ChatBackground({ url }: { url: string }) {
  const { src, onError } = useRemoteImage(url);
  if (!src) return null;
  return (
    <img
      src={src}
      alt=""
      draggable={false}
      className="absolute inset-0 z-[-1] h-full w-full object-cover opacity-40"
      onError={onError}
    />
  );
}

function MediaLightbox({
  preview,
  onClose,
  onDownload,
}: {
  preview: MediaPreview;
  onClose: () => void;
  onDownload: (url: string) => void;
}) {
  const slides: Slide[] = preview.videoUrl
    ? [
        {
          type: "video",
          sources: [{ src: preview.videoUrl, type: "video/mp4" }],
          autoPlay: true,
          loop: preview.loop ?? false,
          muted: preview.muted ?? false,
          download: { url: preview.videoUrl, filename: "media" },
        },
      ]
    : [{ src: preview.imageUrl, download: { url: preview.imageUrl, filename: "media" } }];
  return (
    <Lightbox
      open
      close={onClose}
      slides={slides}
      plugins={[Download, Video, Zoom]}
      video={{ autoPlay: true }}
      carousel={{ finite: true }}
      zoom={{ scrollToZoom: true }}
      labels={{ Download: preview.videoUrl ? "Download video" : "Download" }}
      download={{
        download: ({ slide }) => {
          // SAFETY: `download` is the download payload object when present;
          // string/boolean values carry no URL and fall back to the preview.
          const dl = slide.download as { url: string; filename: string } | null;
          const url = dl ? dl.url : null;
          onDownload(url ?? preview.imageUrl);
        },
      }}
      render={{
        // Images go through useRemoteImage so broken CDN links fall back to
        // the Rust client; the video plugin renders video slides.
        slide: ({ slide }) =>
          slide.type === "video" ? null : <LightboxImage url={slide.src ?? preview.imageUrl} />,
      }}
    />
  );
}

function LightboxImage({ url }: { url: string }) {
  const { src, onError } = useRemoteImage(url);
  return (
    <img
      src={src ?? undefined}
      alt="Media preview"
      draggable={false}
      className="max-h-full max-w-full object-contain"
      onError={onError}
    />
  );
}
