import type { DirectMessage, XmaItem, XmaShare } from "../state";
import { isNumber, type JsonObject } from "./guards";

/** `animated_media` payload (GIF); only the fixed_height variant is used. */
interface AnimatedMedia {
  images?: {
    fixed_height?: {
      mp4?: string;
      url?: string;
      webp?: string;
      width?: string;
      height?: string;
    };
  };
}

export type Attachment =
  | { kind: "media"; thumbUrl: string; videoUrl: string | null; width: number; height: number }
  | {
      kind: "gif";
      videoUrl: string;
      thumbUrl: string;
      width: number;
      height: number;
    }
  | { kind: "voice"; audioUrl: string; durationMs: number; waveform: number[] }
  | {
      kind: "post";
      authorIcon: string;
      authorName: string;
      imageUrl: string;
      title: string;
      subtitle: string;
      targetUrl: string | null;
      /** Feed media pk from `serialized_content_ref`; null when unavailable. */
      mediaId: string | null;
      /** mp4 when the share is a video post; usually null. */
      playableUrl: string | null;
    }
  | {
      kind: "clip";
      imageUrl: string;
      title: string;
      subtitle: string;
      authorName: string;
      authorIcon: string;
      targetUrl: string;
      /** mp4 when the share payload carries it; usually null (see `mediaId`). */
      playableUrl: string | null;
      width: number;
      height: number;
      /** Reel media pk, resolved from `serialized_content_ref`; null when
       * unavailable — the modal then shows only the static preview. */
      mediaId: string | null;
    }
  | {
      kind: "story";
      title: string;
      authorName: string;
      authorIcon: string;
      imageUrl: string;
      targetUrl: string | null;
      playableUrl: string | null;
      /** Story pk from `serialized_content_ref` (`story_igid`). */
      mediaId: string | null;
      /** Story author's user id from the share URL — required to fetch the
       * playable story without marking it seen. */
      ownerId: string | null;
      /** Story expiry (epoch ms) from the share payload; null when absent. */
      expiresAt: number | null;
    }
  | { kind: "sticker"; imageUrl: string }
  | {
      /** Reply-to-note: the quoted note rendered as a pfp circle with the note
       * text in a bubble above it (the reply text renders as the message). */
      kind: "note";
      /** Note author avatar (the note's `preview_url`). */
      authorIcon: string;
      /** Note content (`caption_body_text`). */
      text: string;
    }
  | { kind: "placeholder"; text: string };

/** What the lightbox shows for a media message. `loop`/`muted` mark GIFs. */
export interface MediaPreview {
  imageUrl: string;
  videoUrl: string | null;
  loop?: boolean;
  muted?: boolean;
}

/** The image shown in the full-size preview overlay, plus the video URL
 * (None for photos). Stickers and photos both preview their image. */
export function attachmentPreview(a: Attachment): MediaPreview | null {
  switch (a.kind) {
    case "media":
      return { imageUrl: a.thumbUrl, videoUrl: a.videoUrl };
    case "gif":
      return { imageUrl: a.thumbUrl, videoUrl: a.videoUrl, loop: true, muted: true };
    case "post":
      return { imageUrl: a.imageUrl, videoUrl: null };
    case "clip":
    case "story":
      return { imageUrl: a.imageUrl, videoUrl: a.playableUrl };
    case "sticker":
      return { imageUrl: a.imageUrl, videoUrl: null };
    case "placeholder":
    case "voice":
    case "note":
      return null;
  }
}

/** External link to open (posts link to the original post). */
export function attachmentOpenUrl(a: Attachment): string | null {
  return a.kind === "post" ? a.targetUrl : null;
}

/** Best item by pixel area; entries without a url (wrong-kind payloads) never
 * qualify. Width/height default to 0 when absent. */
function pickBestByArea<
  T extends { url?: string | null; width?: number | null; height?: number | null },
>(items: T[] | undefined): T | null {
  let best: T | null = null;
  let bestArea = -1;
  for (const item of items ?? []) {
    if (!item.url) continue;
    const area = (item.width ?? 0) * (item.height ?? 0);
    if (area > bestArea) {
      bestArea = area;
      best = item;
    }
  }
  return best;
}

/** Best image candidate from an `image_versions2` value (by area). */
function bestCandidate(
  candidates: { width: number; height: number; url: string }[] | undefined,
): { url: string; width: number; height: number } | null {
  return pickBestByArea(candidates);
}

/** Best image candidate URL from an `image_versions2` value (by area). */
export function bestCandidateUrl(
  candidates: { width: number; height: number; url: string }[] | undefined,
): string | null {
  return bestCandidate(candidates)?.url ?? null;
}

/** Best video URL from `video_versions` (largest area wins). */
export function bestVideoUrl(
  versions: { url: string; width: number; height: number }[] | undefined,
): string | null {
  return pickBestByArea(versions)?.url ?? null;
}

function firstPreview(items: XmaItem[] | null | undefined): XmaItem | null {
  return items?.find((it) => it.preview_url) ?? null;
}

/** Media pk from `serialized_content_ref` (`fetch_params.media_igid` for
 * reels/feed, `story_igid` for stories). */
function mediaIdFromRef(x: XmaItem): string | null {
  try {
    const ref = x.serialized_content_ref ? JSON.parse(x.serialized_content_ref) : null;
    return ref?.fetch_params?.media_igid ?? ref?.fetch_params?.story_igid ?? null;
  } catch {
    return null;
  }
}

/** Story author user id from the share's target URL (`reel_owner_id`). */
function storyOwnerFromUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const params = new URL(url).searchParams;
    return params.get("reel_owner_id") ?? params.get("reel_id");
  } catch {
    return null;
  }
}

/** Build a post card from one XMA item (story/reel/feed share). */
function postFromXma(item: XmaItem, caption: string | null): Attachment | null {
  if (!item.preview_url) return null;
  return {
    kind: "post",
    authorIcon: item.header_icon_url ?? "",
    authorName: item.header_title_text ?? "",
    imageUrl: item.preview_url,
    title: caption ?? "",
    subtitle: item.header_subtitle_text ?? item.subtitle_text ?? "",
    targetUrl: item.target_url ?? null,
    mediaId: mediaIdFromRef(item),
    playableUrl: item.playable_url ?? null,
  };
}

type ClipAttachment = Extract<Attachment, { kind: "clip" }>;
type PostAttachment = Extract<Attachment, { kind: "post" }>;
type StoryAttachment = Extract<Attachment, { kind: "story" }>;
export type ShareAttachment = ClipAttachment | PostAttachment | StoryAttachment;

/** Story share card. The playable story is fetched from the author's reel
 * (`feed/user/{uid}/story/`) — which does not mark it seen. */
function storyFromXma(item: XmaItem): Attachment | null {
  if (!item.preview_url) return null;
  return {
    kind: "story",
    title: "",
    authorName: item.header_title_text ?? "",
    authorIcon: item.header_icon_url ?? "",
    imageUrl: item.preview_url,
    targetUrl: item.target_url ?? null,
    playableUrl: item.playable_url ?? null,
    mediaId: mediaIdFromRef(item),
    ownerId: storyOwnerFromUrl(item.target_url),
    expiresAt: isNumber(item.target_expiry_timestamp_ms) ? item.target_expiry_timestamp_ms : null,
  };
}

/** Reel share card. The reel's media pk lives in `serialized_content_ref`
 * (`fetch_params.media_igid`) — that is what the modal fetches full info
 * (video, caption) for. */
function clipFromXma(x: XmaShare): Attachment | null {
  if (!x.preview_url) return null;
  const mediaId = mediaIdFromRef(x);
  return {
    kind: "clip",
    imageUrl: x.preview_url,
    title: x.header_title_text ?? x.title_text ?? x.title ?? "",
    subtitle: x.header_subtitle_text ?? x.subtitle_text ?? "",
    authorName: x.header_title_text ?? "",
    authorIcon: x.header_icon_url ?? "",
    targetUrl: x.target_url ?? "",
    playableUrl: x.playable_url ?? null,
    width: x.preview_width ?? 480,
    height: x.preview_height ?? 854,
    mediaId,
  };
}

/** First `generic_xma` item with a `preview_url`. The unfiltered `raw_xma`
 * copy is checked first — it survives items the extractor prunes. */
function firstGenericXma(msg: DirectMessage): XmaItem | null {
  return (
    firstPreview(msg.raw_xma?.generic_xma) ??
    firstPreview(msg.generic_xma) ??
    null
  );
}

/** Build the `generic_xma` attachment. A non-null `sticker_type` marks a
 * sticker; everything else with note text is a reply-to-note (the note is the
 * `preview_url` avatar, and the reply text renders as its own message bubble). */
function genericXmaAttachment(msg: DirectMessage): Attachment | null {
  const item = firstGenericXma(msg);
  if (!item) return null;
  if (item.sticker_type != null) {
    return { kind: "sticker", imageUrl: item.preview_url ?? "" };
  }
  const text = item.caption_body_text ?? item.title_text ?? "";
  if (text.length > 0) {
    return { kind: "note", authorIcon: item.preview_url ?? "", text };
  }
  return null;
}

/** Resolves a message sender to display data, for shares that carry no
 * author of their own (reel mentions). Returns null when unresolvable. */
export type AuthorResolver = (
  userId: string | null | undefined,
) => { name: string; url: string | null } | null;

/** Extract the rich payload for a message, if its type has one. */
export function attachmentFromMsg(
  msg: DirectMessage,
  resolveAuthor?: AuthorResolver,
): Attachment | null {
  const t = msg.item_type ?? "";
  switch (t) {
    case "media": {
      const media = msg.media;
      if (!media || !media.thumbnail_url) return null;
      return {
        kind: "media",
        thumbUrl: media.thumbnail_url,
        videoUrl: media.video_url ?? null,
        width: media.width ?? 260,
        height: media.height ?? 260,
      };
    }
    case "animated_media": {
      // SAFETY: `animated_media` is the wire GIF payload; only the
      // fixed_height variant is used by this app.
      const am = (msg.animated_media ?? null) as AnimatedMedia | null;
      const img = am?.images?.fixed_height;
      if (!img?.mp4) return null;
      return {
        kind: "gif",
        videoUrl: img.mp4,
        thumbUrl: img.url ?? img.mp4,
        width: Number(img.width) || 150,
        height: Number(img.height) || 200,
      };
    }
    case "voice_media": {
      const media = msg.media;
      if (!media?.audio_url) return null;
      return {
        kind: "voice",
        audioUrl: media.audio_url,
        durationMs: media.audio_duration_ms ?? 0,
        waveform: media.waveform ?? [],
      };
    }
    case "visual_media":
    case "raven_media": {
      const vm = msg.visual_media?.media;
      if (!vm) return null;
      const cand = bestCandidate(vm.image_versions2?.candidates);
      const videoUrl = bestCandidateUrl(vm.video_versions);
      if (!cand) return null;
      return {
        kind: "media",
        thumbUrl: cand.url,
        videoUrl,
        width: cand.width,
        height: cand.height,
      };
    }
    default: {
      // xma_share: reel shares (xma_clip / /reel/ target) get a clip card
      // that opens the detail modal; everything else is a post card.
      if (msg.xma_share) {
        const x = msg.xma_share;
        const raw = msg.raw;
        // SAFETY: raw is the xma payload; `xma_clip` presence marks a reel.
        const isReel =
          x.target_url?.includes("/reel/") || Array.isArray((raw as JsonObject | null)?.xma_clip);
        if (isReel) {
          const clip = clipFromXma(x);
          if (clip) return clip;
        }
        const caption = x.title ?? x.title_text ?? null;
        const post = postFromXma(x, caption);
        if (post) return post;
        // Share-shaped payload with nothing renderable (deleted content or
        // privacy-hidden shares): show the unavailable notice instead of an
        // unsupported row.
        const unavailable = x.caption_body_text ?? x.title_text ?? null;
        if (unavailable) return { kind: "placeholder", text: unavailable };
      }
      // xma_story_share: story card (playable video fetched on open);
      // preview-less items render the unavailable notice.
      for (const item of msg.xma_story_share ?? []) {
        if (item.preview_url) {
          const story = storyFromXma(item);
          if (story) return story;
        } else if (item.caption_body_text || item.title_text) {
          return {
            kind: "placeholder",
            text: item.caption_body_text ?? item.title_text ?? "Message unavailable",
          };
        }
      }
      // xma_reel_mention: post card, caption from `auxiliary_text`. The
      // share carries no author — assume the message sender. Preview-less
      // items (expired/deleted content) render the unavailable notice.
      for (const item of msg.xma_reel_mention ?? []) {
        if (item.preview_url) {
          const post = postFromXma(item, item.auxiliary_text ?? null);
          if (post) {
            if (post.kind === "post") {
              const fallback = resolveAuthor?.(msg.user_id);
              if (fallback) {
                return {
                  ...post,
                  authorName: post.authorName.length > 0 ? post.authorName : fallback.name,
                  authorIcon: post.authorIcon.length > 0 ? post.authorIcon : (fallback.url ?? ""),
                };
              }
            }
            return post;
          }
        } else if (item.caption_body_text || item.title_text) {
          return {
            kind: "placeholder",
            text: item.caption_body_text ?? item.title_text ?? "Message unavailable",
          };
        }
      }
      // Legacy reel shares: item_type `clip` with the media object under
      // `clip` (extracted with video_versions/image_versions2 intact).
      // SAFETY: `clip` is the legacy reel payload; the shape below is what
      // instagrapi extraction keeps. Guarded so non-object clips never enter.
      const clip = msg.clip as {
        id?: string | number;
        code?: string;
        video_url?: string;
        video_versions?: { url: string; width: number; height: number }[];
        image_versions2?: { candidates?: { url: string; width: number; height: number }[] };
        user?: { username?: string; profile_pic_url?: string };
      } | null;
      if (clip) {
        const cand = bestCandidate(clip.image_versions2?.candidates);
        const videoUrl = bestVideoUrl(clip.video_versions) ?? clip.video_url ?? null;
        if (cand || videoUrl) {
          return {
            kind: "clip",
            imageUrl: cand?.url ?? "",
            title: clip.user?.username ?? "",
            subtitle: "",
            authorName: clip.user?.username ?? "",
            authorIcon: clip.user?.profile_pic_url ?? "",
            targetUrl: clip.code ? `https://www.instagram.com/reel/${clip.code}/` : "",
            playableUrl: videoUrl,
            width: cand?.width ?? clip.video_versions?.[0]?.width ?? 480,
            height: cand?.height ?? clip.video_versions?.[0]?.height ?? 854,
            mediaId: clip.id ? String(clip.id).split("_")[0] : null,
          };
        }
      }
      const xma = genericXmaAttachment(msg);
      if (xma) return xma;
      if (msg.placeholder) {
        const text = msg.placeholder.message ?? msg.placeholder.title ?? "Post unavailable";
        return { kind: "placeholder", text };
      }
      return null;
    }
  }
}
