// Wire types mirroring crates/igdm/src/state.rs + instagrapi types (JSON).

import type { Json } from "./lib/guards";

export interface UserShort {
  pk: string;
  username?: string | null;
  full_name?: string | null;
  profile_pic_url?: string | null;
  profile_pic_url_hd?: string | null;
  is_private?: boolean | null;
  is_verified?: boolean | null;
  has_anonymous_profile_picture?: boolean | null;
  latest_reel_media?: number | null;
  profile_pic_id?: string | null;
  fbid_v2?: string | null;
  interop_messaging_user_fbid?: string | null;
  strong_id__?: string | null;
  account_badges?: unknown[];
}

export interface DirectMedia {
  id: string;
  media_type: number;
  user?: UserShort | null;
  thumbnail_url?: string | null;
  video_url?: string | null;
  /** Pixel dimensions of the thumbnail (aspect-ratio rendering). */
  width?: number | null;
  height?: number | null;
  audio_url?: string | null;
  /** Voice message length in milliseconds. */
  audio_duration_ms?: number | null;
  /** Voice message waveform amplitudes, 0..1. */
  waveform?: number[] | null;
}

export interface MessageReaction {
  timestamp: string;
  client_context?: string | null;
  sender_id: number;
  emoji: string;
  super_react_type: string;
}

export interface MessageReactions {
  likes: unknown[];
  likes_count: number;
  emojis: MessageReaction[];
}

/** IG `action_log` payload used to render system rows. */
export interface ActionLog {
  is_reaction_log?: boolean;
  description?: string;
  text_parts?: { text: string }[];
}

export interface MediaShare {
  video_url?: string | null;
  thumbnail_url?: string | null;
}

export interface XmaItem {
  preview_url?: string | null;
  header_icon_url?: string | null;
  header_title_text?: string | null;
  auxiliary_text?: string | null;
  target_url?: string | null;
  playable_url?: string | null;
  preview_width?: number | null;
  preview_height?: number | null;
  header_subtitle_text?: string | null;
  subtitle_text?: string | null;
  title_text?: string | null;
  caption_body_text?: string | null;
  /** Non-null marks a sticker; null means another `generic_xma` kind (the
   * current known case being a reply-to-note, distinguished by `target_url`). */
  sticker_type?: string | null;
  /** JSON string with `{ fetch_params: { media_igid } }` for reel/feed shares. */
  serialized_content_ref?: string | null;
  /** Story expiry, epoch ms (stories only). */
  target_expiry_timestamp_ms?: number | null;
}

export interface XmaShare extends XmaItem {
  title?: string | null;
  title_text?: string | null;
  video_url?: string | null;
  thumbnail_url?: string | null;
}

export interface VisualMedia {
  media?: {
    image_versions2?: { candidates?: { width: number; height: number; url: string }[] };
    video_versions?: { width: number; height: number; url: string }[];
  } | null;
}

export interface Placeholder {
  message?: string | null;
  title?: string | null;
}

export interface DirectMessage {
  id: string;
  user_id?: string | null;
  /** thread id as string (can exceed u64; safe as a map key). */
  thread_id?: string | null;
  /** RFC3339 local timestamp (DateTime<Local>). */
  timestamp: string;
  item_type?: string | null;
  is_sent_by_viewer?: boolean | null;
  is_shh_mode?: boolean | null;
  reactions?: MessageReactions | null;
  text?: string | null;
  reply?: DirectMessage | null;
  link?: unknown;
  animated_media?: unknown;
  media?: DirectMedia | null;
  visual_media?: VisualMedia | null;
  media_share?: MediaShare | null;
  reel_share?: unknown;
  story_share?: unknown;
  felix_share?: unknown;
  xma_share?: XmaShare | null;
  generic_xma?: XmaItem[] | null;
  raw_xma?: { generic_xma?: XmaItem[] } | null;
  clip?: unknown;
  placeholder?: Placeholder | null;
  xma_story_share?: XmaItem[] | null;
  xma_reel_mention?: XmaItem[] | null;
  action_log?: ActionLog | null;
  client_context?: string | null;
  raw?: Json;
}

export interface LastSeenInfo {
  item_id?: string | null;
  timestamp?: string | null;
  created_at?: string | null;
  shh_seen_state?: unknown;
  disappearing_messages_seen_state?: unknown;
}

export interface DirectThread {
  pk: string;
  id: string;
  messages: DirectMessage[];
  users: UserShort[];
  inviter?: UserShort | null;
  left_users: UserShort[];
  admin_user_ids: unknown[];
  last_activity_at: string;
  muted: boolean;
  is_pin?: boolean | null;
  named: boolean;
  canonical: boolean;
  pending: boolean;
  archived: boolean;
  thread_type: string;
  thread_title: string;
  folder: number;
  vc_muted: boolean;
  is_group: boolean;
  mentions_muted: boolean;
  approval_required_for_new_members: boolean;
  input_mode: number;
  business_thread_folder?: number | null;
  read_state?: number | null;
  is_close_friend_thread: boolean;
  assigned_admin_id?: number | null;
  shh_mode_enabled?: boolean | null;
  last_seen_at: Record<string, LastSeenInfo>;
  /** Raw IG chat theme (`theme_data`), passed through from the thread payload. */
  theme_data?: ThreadTheme | null;
}

/** IG thread theme (`theme_data`). Colors are 8-digit ARGB hex. */
export interface ThreadTheme {
  app_color_mode?: string | null;
  incoming_message_bubble_color?: string | null;
  inbound_message_text_color?: string | null;
  outbound_message_text_color?: string | null;
  quoted_incoming_message_bubble_color?: string | null;
  quoted_incoming_message_text_color?: string | null;
  reaction_pill_color?: string | null;
  secondary_text_color?: string | null;
  solid_separator_color?: string | null;
  navigation_bar_color?: string | null;
  navigation_bar_title_color?: string | null;
  navigation_bar_subtitle_color?: string | null;
  navigation_bar_icon_color?: string | null;
  composer_input_background_color?: string | null;
  composer_placeholder_text_color?: string | null;
  composer_secondary_button_color?: string | null;
  composer_send_button_colors?: string[] | null;
  composer_circle_button_colors?: string[] | null;
  should_use_diagonal_gradient_for_composer_circle_button?: boolean | null;
  emphasis_colors?: string[] | null;
  emphasized_action_color?: string | null;
  blurred_composer_background_color?: string | null;
  blurred_composer_opaque_background_color?: string | null;
  solid_composer_background_color?: string | null;
  gradient_colors?: string[] | null;
  fallback_color?: string | null;
  thread_background_color?: string | null;
  thread_background_asset?: {
    four_hundred_eighty?: string | null;
    seven_hundred_twenty?: string | null;
    one_thousand_twenty_four?: string | null;
    two_thousand_forty_eight?: string | null;
  } | null;
  alternative_themes?: ThreadTheme[] | null;
}

// ------------------------------------------------------------------ wire model

export interface ThreadMeta {
  nicknames: Record<string, string>;
  avatar: string;
}

export interface MeInfo {
  username: string;
  user_id: string;
  profile_pic_url: string;
}

export interface LiveMessage {
  thread_id: string;
  item_id: string;
  op: string;
  user_id: string;
  text: string | null;
  timestamp: number;
  item_type: string;
  message: DirectMessage | null;
}

/** Decoded backend event (see `decodeEvent` in lib/events.ts for the wire
 * mapping — the Rust side serializes tuple variants as JSON arrays). */
export type AppEvent =
  | { type: "Status"; connected: boolean; detail: string }
  | { type: "LoginError"; text: string }
  | { type: "LoggedIn"; me: MeInfo }
  | { type: "LoggedOut" }
  | { type: "CodePrompt"; text: string }
  | { type: "LiveMessage"; live: LiveMessage }
  | { type: "Typing"; threadId: string; senderId: string; active: boolean }
  | { type: "Seen"; threadId: string; userId: string; itemId: string }
  | { type: "ThreadsLoaded"; threads: DirectThread[]; meta: Record<string, ThreadMeta> }
  | { type: "ThreadDetails"; threadId: string; thread: DirectThread; meta: ThreadMeta }
  | {
      type: "MessagesLoaded";
      threadId: string;
      messages: DirectMessage[];
      cursor: string | null;
      hasMore: boolean;
    }
  | {
      type: "OlderLoaded";
      threadId: string;
      messages: DirectMessage[];
      cursor: string | null;
      hasMore: boolean;
    }
  | { type: "Sent"; key: string; realThreadId: string; msg: DirectMessage }
  | { type: "SendFailed"; key: string; text: string }
  | { type: "SearchResults"; query: string; users: UserShort[] }
  | { type: "SearchFailed"; query: string }
  | { type: "ThreadByUser"; user: UserShort; threadId: string | null }
  | { type: "Approved"; key: string }
  | { type: "MediaDone"; path: string }
  | { type: "MediaFailed"; text: string };

// --------------------------------------------------------------- app state

export interface ReplyInfo {
  threadKey: string;
  msgId: string;
  /** Target message's client context (IG requires it for replies). */
  clientContext: string | null;
  /** Short "sender: text" preview shown above the composer. */
  preview: string;
}

export interface ThreadState {
  /** Real thread id, or `user:<pk>` for a new 1:1 chat. */
  key: string;
  title: string;
  users: UserShort[];
  /** Ascending by timestamp. */
  messages: DirectMessage[];
  oldest_cursor: string | null;
  has_more: boolean;
  loaded: boolean;
  unread: boolean;
  is_group: boolean;
  pending: boolean;
  /** epoch seconds */
  last_activity: number;
  /** user_id -> typing expiry (epoch seconds) */
  typing: Record<string, number>;
  /** user_id -> last seen item id */
  seen_by: Record<string, string>;
  /** user_id -> nickname (per-thread) */
  nicknames: Record<string, string>;
  /** Group thread avatar url */
  avatar: string;
  meta_fetching: boolean;
  loading_older: boolean;
  /** read_state from the thread payload */
  read_state: number;
  /** user_id -> last seen timestamp (epoch seconds) */
  last_seen_at: Record<string, number>;
  /** Raw IG chat theme, applied to the message pane when enabled. */
  theme_data: ThreadTheme | null;
}

export function emptyThreadState(key: string): ThreadState {
  return {
    key,
    title: "",
    users: [],
    messages: [],
    oldest_cursor: null,
    has_more: false,
    loaded: false,
    unread: false,
    is_group: false,
    pending: false,
    last_activity: 0,
    typing: {},
    seen_by: {},
    nicknames: {},
    avatar: "",
    meta_fetching: false,
    loading_older: false,
    read_state: 0,
    last_seen_at: {},
    theme_data: null,
  };
}

export function displayName(ts: ThreadState, user: UserShort): string {
  const nick = ts.nicknames[user.pk];
  return nick || (user.full_name ? user.full_name : "") || user.username || "?";
}

/** Sidebar/header title: nickname for 1:1, group name for groups. */
export function threadTitle(ts: ThreadState): string {
  if (!ts.is_group && ts.users.length > 0) {
    return displayName(ts, ts.users[0]);
  }
  return ts.title ? ts.title : "Unknown";
}

/** Display name for a bare user (no thread context). */
export function titleForUser(user: UserShort): string {
  return user.full_name && user.full_name.trim().length > 0 ? user.full_name : user.username || "?";
}

/** Inbox title for a wire thread: explicit title, else joined user names. */
export function threadTitleFrom(thread: DirectThread): string {
  return thread.thread_title.length > 0
    ? thread.thread_title
    : thread.users.map(titleForUser).join(", ");
}

/** Avatar url + fallback name for a thread (header, sidebar row). */
export interface ThreadAvatar {
  url: string | null;
  name: string;
}

/** Avatar url + fallback name for a thread (header, sidebar row). */
export function avatarForThread(ts: ThreadState): ThreadAvatar {
  if (ts.is_group) {
    return { url: ts.avatar || null, name: ts.title };
  }
  const user = ts.users[0];
  if (user) {
    return {
      url: user.profile_pic_url || null,
      name: ts.nicknames[user.pk] || user.full_name || user.username || threadTitle(ts),
    };
  }
  return { url: null, name: threadTitle(ts) };
}

/** instagrapi's is_seen semantics are inverted; compute from read_state and
 * per-user seen timestamps. */
export function unreadFor(ts: ThreadState, viewerId: string): boolean {
  if (ts.read_state === 1) return true;
  const meTs = ts.last_seen_at[viewerId] ?? 0;
  return Object.entries(ts.last_seen_at).some(([uid, t]) => uid !== viewerId && t > meTs);
}

export function sortMessages(ts: ThreadState): ThreadState {
  if (ts.messages.length < 2) return ts;
  const messages = [...ts.messages].toSorted((a, b) => tsMillis(a) - tsMillis(b));
  return { ...ts, messages };
}

/** DirectMessage.timestamp (RFC3339) -> epoch millis. */
export function tsMillis(msg: DirectMessage): number {
  return new Date(msg.timestamp).getTime();
}

export interface LoginState {
  username: string;
  password: string;
  sessionid: string;
  error: string;
  busy: boolean;
  /** Saved session awaiting the backend result (drives the row spinner). */
  pendingSession: string | null;
  code_info: string;
  show_code: boolean;
}

export const DEFAULT_LOGIN_STATE: LoginState = {
  username: "",
  password: "",
  sessionid: "",
  error: "",
  busy: false,
  pendingSession: null,
  code_info: "",
  show_code: false,
};

export interface AppState {
  /** `booting` = resume/auto-login splash while the last-used session loads. */
  screen: "booting" | "login" | "main";
  login: LoginState;
  me: MeInfo;
  threads: Record<string, ThreadState>;
  openKey: string | null;
  searchQuery: string;
  searchResults: UserShort[];
  showSearch: boolean;
  searching: boolean;
  connected: boolean;
  statusDetail: string;
  /** thread_id -> raw inbox/thread JSON (copy raw data menu) */
  threadRaw: Record<string, Json>;
  /** Reaction emojis shown in the message context menu (editable). */
  reactionEmojis: string[];
  /** Active reply target for the composer. */
  reply: ReplyInfo | null;
  /** Pending scroll-to target (message id) requested by clicking a reply preview. */
  replyScroll: string | null;
  /** Remaining attempts to load history looking for the reply target. */
  replyRetries: number;
  /** saved session names for the login screen */
  savedSessions: string[];
  /** True while the inbox list is being fetched (drives skeletons). */
  inboxLoading: boolean;
  /** Master switch for per-thread IG chat themes. */
  chatThemesEnabled: boolean;
}

const DEFAULT_REACTION_EMOJIS = ["❤️", "😆", "😮", "😢", "😡"];

export function defaultAppState(): AppState {
  return {
    screen: "booting",
    login: { ...DEFAULT_LOGIN_STATE },
    me: { username: "", user_id: "", profile_pic_url: "" },
    threads: {},
    openKey: null,
    searchQuery: "",
    searchResults: [],
    showSearch: false,
    searching: false,
    connected: false,
    statusDetail: "",
    threadRaw: {},
    reactionEmojis: [...DEFAULT_REACTION_EMOJIS],
    reply: null,
    replyScroll: null,
    replyRetries: 0,
    savedSessions: [],
    inboxLoading: false,
    chatThemesEnabled: true,
  };
}
