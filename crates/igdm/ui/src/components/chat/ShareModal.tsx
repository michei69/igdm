import { useCallback, useEffect, useMemo, useRef, useState, type UIEvent } from "react";
import { LazyMotion, domAnimation, m, useReducedMotion, type MotionProps } from "motion/react";
import { api } from "../../lib/api";
import { bestCandidateUrl, bestVideoUrl, type ShareAttachment } from "../../lib/attachment";
import { isNumber, isString, type JsonObject } from "../../lib/guards";
import ShareMediaCarousel, { type Slide } from "./ShareMediaCarousel";
import ShareCommentsPanel, { type ShareComment } from "./ShareCommentsPanel";

function useExpiredFlag(mediaId: string | null) {
  const [state, setState] = useState({ mediaId, expired: false });
  if (state.mediaId !== mediaId) {
    setState({ mediaId, expired: false });
  }
  const setExpired = useCallback((val: boolean) => {
    setState((prev) => ({ ...prev, expired: val }));
  }, []);
  return [state.expired, setExpired] as const;
}

interface MediaVersion {
  url: string;
  width: number;
  height: number;
}

interface CarouselMedia {
  media_type?: number;
  video_versions?: MediaVersion[];
  image_versions2?: { candidates?: MediaVersion[] };
}

interface ShareInfo {
  media_type?: number;
  video_versions?: MediaVersion[];
  image_versions2?: { candidates?: MediaVersion[] };
  carousel_media?: CarouselMedia[];
  caption_text?: string | null;
  user?: { username?: string; full_name?: string; profile_pic_url?: string } | null;
  comment_count?: number | null;
  play_count?: number | null;
  like_count?: number | null;
}

/** Carousel slides from full media info; single slide before/without it. */
function slidesFromInfo(
  info: ShareInfo | null,
  fallback: { playableUrl: string | null; imageUrl: string },
): Slide[] {
  const carousel = info?.carousel_media;
  if (Array.isArray(carousel) && carousel.length > 0) {
    return carousel.map((media) => ({
      videoUrl: bestVideoUrl(media.video_versions),
      imageUrl: bestCandidateUrl(media.image_versions2?.candidates),
    }));
  }
  return [
    {
      videoUrl: info
        ? (bestVideoUrl(info.video_versions) ?? fallback.playableUrl)
        : fallback.playableUrl,
      imageUrl: info
        ? (bestCandidateUrl(info.image_versions2?.candidates) ?? fallback.imageUrl)
        : fallback.imageUrl,
    },
  ];
}

interface Props {
  share: ShareAttachment;
  onClose: () => void;
  onDownload: (url: string) => void;
  onOpenWeb: (url: string) => void;
}

function statsFor(info: ShareInfo | null): string[] {
  const stats: string[] = [];
  if (isNumber(info?.play_count)) stats.push(`${info.play_count.toLocaleString()} plays`);
  if (isNumber(info?.like_count)) stats.push(`${info.like_count.toLocaleString()} likes`);
  if (isNumber(info?.comment_count)) stats.push(`${info.comment_count.toLocaleString()} comments`);
  return stats;
}

/** Fetch the full media (video/carousel/caption) from the media pk. Stories
 * resolve through the author's reel, which does not mark seen. */
function useShareInfo(share: ShareAttachment) {
  const [info, setInfo] = useState<ShareInfo | null>(null);
  const [expired, setExpired] = useExpiredFlag(share.mediaId);
  useEffect(() => {
    if (!share.mediaId) return;
    let cancelled = false;
    if (share.kind === "story") {
      const ownerId = share.ownerId;
      if (!ownerId) return;
      const expiresAt = share.expiresAt;
      if (expiresAt !== null && Date.now() > expiresAt) {
        // Already past the payload expiry: don't bother fetching.
        setExpired(true);
        return;
      }
      api
        .storyInfo(share.mediaId, ownerId)
        .then((raw) => {
          if (cancelled) return;
          // SAFETY: the story payload is a ShareInfo-shaped wire object.
          const obj = raw as JsonObject | null;
          if (obj) {
            // SAFETY: when raw is an object it carries the ShareInfo fields.
            setInfo(raw as ShareInfo);
          } else {
            // The reel no longer contains the story: it expired.
            setExpired(true);
          }
        })
        .catch(() => setExpired(true));
    } else {
      api
        .reelInfo(share.mediaId)
        .then((raw) => {
          if (cancelled) return;
          // SAFETY: the reel payload is a ShareInfo-shaped wire object.
          const obj = raw as JsonObject | null;
          if (obj) {
            // SAFETY: when raw is an object it carries the ShareInfo fields.
            setInfo(raw as ShareInfo);
          }
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [share.mediaId, share.kind, setExpired, share]);
  return { info, expired };
}

/** One comments page at a time; `maxId` appends, `null` replaces (first page).
 * Uses refs for the infinite-scroll state so it never triggers a re-render. */
function useShareComments(mediaId: string | null) {
  const [comments, setComments] = useState<ShareComment[]>([]);
  const [loading, setLoading] = useState(true);
  const loadingMoreRef = useRef(false);
  const nextMaxIdRef = useRef<string | null>(null);
  const hasMoreRef = useRef(true);

  const loadPage = useCallback(
    (maxId: string | null) => {
      if (!mediaId) return;
      if (maxId) loadingMoreRef.current = true;
      api
        .mediaComments(mediaId, maxId)
        .then((raw) => {
          // SAFETY: the comments payload is object-or-null from the backend.
          const obj = raw as JsonObject | null;
          if (!obj || !("comments" in obj) || !Array.isArray(obj.comments)) {
            hasMoreRef.current = false;
            return;
          }
          // SAFETY: the backend comments page is a ShareComment[] payload.
          const list = obj.comments as ShareComment[];
          const next = "next_max_id" in obj && isString(obj.next_max_id) ? obj.next_max_id : null;
          const more =
            "has_more_comments" in obj && obj.has_more_comments === true && next !== null;
          setComments((prev) => (maxId ? [...prev, ...list] : list));
          nextMaxIdRef.current = next;
          hasMoreRef.current = more;
        })
        .catch(() => {
          hasMoreRef.current = false;
        })
        .finally(() => {
          setLoading(false);
          loadingMoreRef.current = false;
        });
    },
    [mediaId],
  );

  // First page on media change.
  useEffect(() => {
    setComments([]);
    nextMaxIdRef.current = null;
    hasMoreRef.current = true;
    setLoading(true);
    loadPage(null);
  }, [loadPage]);

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    if (el.scrollTop + el.clientHeight < el.scrollHeight - 40) return;
    if (hasMoreRef.current && !loadingMoreRef.current && nextMaxIdRef.current) {
      loadPage(nextMaxIdRef.current);
    }
  };

  return { comments, loading, loadingMore: loadingMoreRef.current, onScroll };
}

function dialogMotion(
  closing: boolean,
  reduce: boolean | null,
): Pick<MotionProps, "initial" | "animate" | "transition"> {
  return {
    initial: reduce ? { opacity: 0 } : { opacity: 0, scale: 0.92, y: 14 },
    animate: closing
      ? reduce
        ? { opacity: 0 }
        : { opacity: 0, scale: 0.95, y: 8 }
      : reduce
        ? { opacity: 1 }
        : { opacity: 1, scale: 1, y: 0 },
    transition: closing
      ? reduce
        ? { duration: 0 }
        : { duration: 0.15, ease: "easeIn" }
      : reduce
        ? { duration: 0 }
        : { type: "spring", stiffness: 420, damping: 32, mass: 0.9 },
  };
}

function ShareDialogContent({
  share,
  slides,
  slide,
  expired,
  comments,
  commentCount,
  commentsLoading,
  loadingMore,
  onCommentsScroll,
  username,
  caption,
  stats,
  downloadUrl,
  onDownload,
  onOpenWeb,
  onPrev,
  onNext,
  onClose,
}: {
  share: ShareAttachment;
  slides: Slide[];
  slide: number;
  expired: boolean;
  comments: ShareComment[];
  commentCount: number;
  commentsLoading: boolean;
  loadingMore: boolean;
  onCommentsScroll: (e: UIEvent<HTMLDivElement>) => void;
  username: string;
  caption: string;
  stats: string[];
  downloadUrl: string;
  onDownload: (url: string) => void;
  onOpenWeb: (url: string) => void;
  onPrev: () => void;
  onNext: () => void;
  onClose: () => void;
}) {
  return (
    <>
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-[15px] font-bold text-ink">
          {share.authorName ||
            (share.kind === "clip" ? "Reel" : share.kind === "story" ? "Story" : "Post")}
        </span>
        <button
          className="shrink-0 rounded-md px-1.5 text-[16px] leading-none text-ink3 hover:bg-panel2"
          onClick={onClose}
          aria-label="Close"
        >
          ✕
        </button>
      </div>

      <div className="flex min-h-0 gap-3">
        <ShareMediaCarousel
          slides={slides}
          currentIndex={slide}
          expired={expired}
          onPrevious={onPrev}
          onNext={onNext}
        />

        {/* Comments panel (stories have no comments) */}
        {share.kind !== "story" && share.mediaId && (
          <ShareCommentsPanel
            comments={comments}
            commentCount={commentCount}
            loading={commentsLoading}
            loadingMore={loadingMore}
            onScroll={onCommentsScroll}
          />
        )}
      </div>

      <div className="flex flex-col gap-0.5">
        <span className="text-[13px] font-medium text-ink">{username}</span>
        {share.kind !== "story" && share.subtitle.length > 0 && (
          <span className="text-[12px] text-ink2">{share.subtitle}</span>
        )}
        {caption.length > 0 && (
          <span className="line-clamp-3 text-[12px] text-ink2">{caption}</span>
        )}
        {stats.length > 0 && <span className="text-[11px] text-ink3">{stats.join(" · ")}</span>}
      </div>

      <div className="flex gap-2">
        <button
          className="flex h-9 flex-1 items-center justify-center rounded-lg bg-accent text-[13px] font-medium text-white hover:bg-accent-hover"
          onClick={() => onDownload(downloadUrl)}
        >
          Download
        </button>
        <button
          className="flex h-9 flex-1 items-center justify-center rounded-lg border border-border text-[13px] font-medium text-ink hover:bg-panel2 disabled:opacity-50"
          disabled={!share.targetUrl}
          onClick={() => {
            if (share.targetUrl) onOpenWeb(share.targetUrl);
          }}
        >
          Open in web
        </button>
      </div>
    </>
  );
}

export default function ShareModal({ share, onClose, onDownload, onOpenWeb }: Props) {
  const reduce = useReducedMotion();
  const { info, expired } = useShareInfo(share);
  const {
    comments,
    loading: commentsLoading,
    loadingMore,
    onScroll: onCommentsScroll,
  } = useShareComments(share.mediaId);
  // Self-managed exit: play the closing animation first, then unmount via
  // onClose — the parent never removes a still-animating element.
  const [closing, setClosing] = useState(false);

  // Derive slide index and reset from share.mediaId changes
  const [slideState, setSlideState] = useState({ mediaId: share.mediaId, index: 0 });
  const slide = slideState.mediaId === share.mediaId ? slideState.index : 0;

  const slides = useMemo(
    () => slidesFromInfo(info, { playableUrl: share.playableUrl, imageUrl: share.imageUrl }),
    [info, share.playableUrl, share.imageUrl],
  );

  // Keyboard: Escape closes, arrows move the carousel.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setClosing(true);
      else if (e.key === "ArrowLeft")
        setSlideState((s) => (s.index > 0 ? { ...s, index: s.index - 1 } : s));
      else if (e.key === "ArrowRight")
        setSlideState((s) => (s.index < slides.length - 1 ? { ...s, index: s.index + 1 } : s));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [slides.length]);

  const username = info?.user?.username ?? share.authorName;
  const caption = info?.caption_text ?? share.title;
  const stats = statsFor(info);
  const current = slides[Math.min(slide, slides.length - 1)];
  const downloadUrl = current.videoUrl ?? current.imageUrl ?? share.imageUrl;
  const commentCount = isNumber(info?.comment_count) ? info.comment_count : comments.length;

  return (
    <LazyMotion features={domAnimation}>
      <m.div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
        initial={{ opacity: 0 }}
        animate={{ opacity: closing ? 0 : 1 }}
        transition={{ duration: reduce ? 0 : 0.15 }}
        onClick={() => setClosing(true)}
      >
        <m.div
          className="flex max-h-full w-full max-w-[720px] flex-col gap-3 overflow-hidden rounded-xl border border-border bg-elevated p-4 shadow-2xl"
          {...dialogMotion(closing, reduce)}
          onAnimationComplete={() => {
            if (closing) onClose();
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <ShareDialogContent
            share={share}
            slides={slides}
            slide={slide}
            expired={expired}
            comments={comments}
            commentCount={commentCount}
            commentsLoading={commentsLoading}
            loadingMore={loadingMore}
            onCommentsScroll={onCommentsScroll}
            username={username}
            caption={caption}
            stats={stats}
            downloadUrl={downloadUrl}
            onDownload={onDownload}
            onOpenWeb={onOpenWeb}
            onPrev={() => setSlideState((s) => ({ ...s, index: s.index - 1 }))}
            onNext={() => setSlideState((s) => ({ ...s, index: s.index + 1 }))}
            onClose={() => setClosing(true)}
          />
        </m.div>
      </m.div>
    </LazyMotion>
  );
}
