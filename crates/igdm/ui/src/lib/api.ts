// Tauri command wrappers (mirror crates/igdm/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type { UserShort } from "../state";
import type { Json } from "./guards";

interface BootstrapData {
  saved_sessions: string[];
  reaction_emojis: string[];
  theme: string;
  chat_themes: boolean;
}

export interface ReplyRef {
  message_id: string;
  client_context?: string | null;
}

export const api = {
  getBootstrap: () => invoke<BootstrapData>("get_bootstrap"),
  loginPassword: (username: string, password: string) =>
    invoke<void>("login_password", { username, password }),
  loginSessionid: (sessionid: string) => invoke<void>("login_sessionid", { sessionid }),
  loginSaved: (name: string) => invoke<void>("login_saved", { name }),
  provideCode: (code: string) => invoke<void>("provide_code", { code }),
  cancelCode: () => invoke<void>("cancel_code"),
  logout: () => invoke<void>("logout"),
  refreshInbox: () => invoke<void>("refresh_inbox"),
  loadMessages: (threadId: string, amount: number) =>
    invoke<void>("load_messages", { threadId, amount }),
  loadOlder: (threadId: string, cursor: string) => invoke<void>("load_older", { threadId, cursor }),
  threadDetails: (threadId: string) => invoke<void>("thread_details", { threadId }),
  threadRaw: (threadId: string) => invoke<Json | null>("thread_raw", { threadId }),
  reelInfo: (mediaId: string) => invoke<unknown>("reel_info", { mediaId }),
  mediaComments: (mediaId: string, maxId?: string | null) =>
    invoke<unknown>("media_comments", { mediaId, maxId }),
  storyInfo: (storyId: string, ownerId: string) =>
    invoke<unknown>("story_info", { storyId, ownerId }),
  approveRequest: (threadId: string) => invoke<void>("approve_request", { threadId }),
  searchUsers: (query: string) => invoke<void>("search_users", { query }),
  threadForUser: (user: UserShort) => invoke<void>("thread_for_user", { user }),
  getReactionEmojis: () => invoke<string[]>("get_reaction_emojis"),
  saveReactionEmojis: (emojis: string[]) => invoke<void>("save_reaction_emojis", { emojis }),
  getTheme: () => invoke<string>("get_theme"),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  getChatThemes: () => invoke<boolean>("get_chat_themes"),
  setChatThemes: (enabled: boolean) => invoke<void>("set_chat_themes", { enabled }),
  sendText: (threadId: string, text: string, userIds: string[], replyTo: ReplyRef | null) =>
    invoke<void>("send_text", { threadId, text, userIds, replyTo }),
  sendPhoto: (threadId: string, path: string) => invoke<void>("send_photo", { threadId, path }),
  sendPhotoBytes: (threadId: string, data: Uint8Array, ext: string) =>
    invoke<void>("send_photo_bytes", { threadId, data, ext }),
  sendVideo: (threadId: string, path: string) => invoke<void>("send_video", { threadId, path }),
  sendReaction: (threadId: string, messageId: string, emoji: string, del: boolean) =>
    invoke<void>("send_reaction", { threadId, messageId, emoji, delete: del }),
  sendVoice: (threadId: string, data: Uint8Array, ext: string) =>
    invoke<void>("send_voice", { threadId, data, ext }),
  markSeen: (threadId: string, itemId: string, raw?: Json) =>
    invoke<void>("mark_seen", { threadId, itemId, raw }),
  sendTyping: (threadId: string, active: boolean) =>
    invoke<void>("send_typing", { threadId, active }),
  downloadMedia: (url: string) => invoke<void>("download_media", { url }),
  copyLargeText: (text: string) => invoke<void>("copy_large_text", { text }),
};
