// AppProvider: holds AppState, subscribes to backend events, applies the
// reducer, runs returned effects, and exposes UI actions.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import type {
  AppEvent,
  AppState,
  DirectMessage,
  ReplyInfo,
  ThreadState,
  UserShort,
} from "../state";
import { defaultAppState, emptyThreadState, sortMessages, tsMillis } from "../state";
import { api, type ReplyRef } from "../lib/api";
import { subscribeEvents } from "../lib/events";
import { applyEvent, type Effect } from "../lib/reducer";
import { applyTheme, isTheme } from "../lib/theme";
import { clearImageCache } from "./useRemoteImage";
import type { Json } from "../lib/guards";

interface AppContextValue {
  state: AppState;
  // login
  loginPassword: (username: string, password: string) => void;
  loginSessionid: (sessionid: string) => void;
  loginSaved: (name: string) => void;
  provideCode: (code: string) => void;
  cancelCode: () => void;
  logout: () => void;
  // data
  refreshInbox: () => void;
  loadMessages: (threadId: string, amount?: number) => void;
  loadOlder: (threadId: string, cursor: string) => void;
  threadDetails: (threadId: string) => void;
  threadRaw: (threadId: string) => Promise<Json | null>;
  approveRequest: (threadId: string) => void;
  searchUsers: (query: string) => void;
  threadForUser: (user: UserShort) => void;
  saveReactionEmojis: (emojis: string[]) => void;
  openThread: (key: string) => void;
  openPendingThread: (key: string) => void;
  // actions
  sendText: (threadId: string, text: string, userIds: string[], replyTo: ReplyRef | null) => void;
  sendPhoto: (threadId: string, path: string) => void;
  sendPhotoBytes: (threadId: string, data: Uint8Array, ext: string) => void;
  sendVideo: (threadId: string, path: string) => void;
  sendVoice: (threadId: string, data: Uint8Array, ext: string) => void;
  sendReaction: (threadId: string, messageId: string, emoji: string, del: boolean) => void;
  markSeen: (threadId: string, itemId: string, raw?: Json) => void;
  sendTyping: (threadId: string, active: boolean) => void;
  downloadMedia: (url: string) => void;
  // local UI state mutations
  setSearch: (query: string) => void;
  clearSearch: () => void;
  setReply: (reply: ReplyInfo | null) => void;
  setReplyScroll: (msgId: string | null, retries: number) => void;
  updateThread: (key: string, fn: (ts: ThreadState) => ThreadState) => void;
}

const AppContext = createContext<AppContextValue | null>(null);

function runEffect(effect: Effect): void {
  switch (effect.kind) {
    case "refresh_inbox":
      api.refreshInbox();
      break;
    case "thread_details":
      api.threadDetails(effect.threadId);
      break;
    case "mark_seen":
      api.markSeen(effect.threadId, effect.itemId, effect.raw);
      break;
    case "load_older":
      api.loadOlder(effect.threadId, effect.cursor);
      break;
    case "reveal_media":
      revealItemInDir(effect.path);
      break;
    case "toast":
      toast(effect.text);
      break;
    case "notify": {
      // The reducer emits a notify effect for every new incoming message;
      // the focus check lives here because it is a runtime concern.
      if (document.hasFocus()) break;
      void notifyMessage(effect.title, effect.body);
      break;
    }
  }
}

/** Send a system notification, requesting permission on first use. */
async function notifyMessage(title: string, body: string): Promise<void> {
  try {
    if (!(await isPermissionGranted())) {
      if ((await requestPermission()) !== "granted") return;
    }
    sendNotification({ title, body });
  } catch (err) {
    console.error("notification failed:", err);
  }
}

// Identity-stable actions: pure `api.*` passthroughs need no component state,
// so they live at module level and keep a constant identity across renders.
const provideCode = (code: string) => api.provideCode(code);
const cancelCode = () => api.cancelCode();
const logout = () => api.logout();
const loadMessages = (threadId: string, amount = 30) => api.loadMessages(threadId, amount);
const loadOlder = (threadId: string, cursor: string) => api.loadOlder(threadId, cursor);
const threadDetails = (threadId: string) => api.threadDetails(threadId);
const threadRaw = (threadId: string) => api.threadRaw(threadId);
const approveRequest = (threadId: string) => api.approveRequest(threadId);
const searchUsers = (query: string) => api.searchUsers(query);
const threadForUser = (user: UserShort) => api.threadForUser(user);
const sendPhoto = (threadId: string, path: string) => {
  api.sendPhoto(threadId, path).catch((e) => toast(`Send failed: ${e}`));
};
const sendPhotoBytes = (threadId: string, data: Uint8Array, ext: string) => {
  api.sendPhotoBytes(threadId, data, ext).catch((e) => toast(`Send failed: ${e}`));
};
const sendVideo = (threadId: string, path: string) => {
  api.sendVideo(threadId, path).catch((e) => toast(`Send failed: ${e}`));
};
const sendVoice = (threadId: string, data: Uint8Array, ext: string) => {
  api.sendVoice(threadId, data, ext).catch((e) => toast(`Send failed: ${e}`));
};
const sendReaction = (threadId: string, messageId: string, emoji: string, del: boolean) =>
  api.sendReaction(threadId, messageId, emoji, del);
const markSeen = (threadId: string, itemId: string, raw?: Json) =>
  api.markSeen(threadId, itemId, raw);
const sendTyping = (threadId: string, active: boolean) => api.sendTyping(threadId, active);
const downloadMedia = (url: string) => api.downloadMedia(url);

export function AppProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AppState>(defaultAppState);
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);
  // React StrictMode dev re-runs the bootstrap effect; the guard keeps the
  // launch resume (a live login) from firing twice.
  const autoLoginRef = useRef(false);

  // Subscribe to backend events first so no event — notably the `LoggedIn`
  // that completes a launch resume — can be missed, then load the saved
  // sessions/theme and resume the most recently-used session.
  useEffect(() => {
    let disposed = false;
    let unlistenEvents: (() => void) | null = null;
    let unlistenTheme: (() => void) | null = null;
    let unlistenChatThemes: (() => void) | null = null;

    subscribeEvents((event: AppEvent) => {
      if (disposed) return;
      if (event.type === "LoggedOut") clearImageCache();
      const result = applyEvent(stateRef.current, event);
      stateRef.current = result.state;
      setState(result.state);
      for (const effect of result.effects) {
        runEffect(effect);
      }
    }).then((u) => {
      if (disposed) {
        u();
        return;
      }
      unlistenEvents = u;
      // Events are flowing now, so a resume that emits LoggedIn/LoginError is
      // guaranteed to reach the reducer.
      api
        .getBootstrap()
        .then((b) => {
          if (disposed) return;
          setState((s) => ({
            ...s,
            savedSessions: b.saved_sessions,
            reactionEmojis: b.reaction_emojis.length === 5 ? b.reaction_emojis : s.reactionEmojis,
            chatThemesEnabled: b.chat_themes,
          }));
          if (isTheme(b.theme)) applyTheme(b.theme);
          // Resume the most recently-used saved session (the backend returns
          // `saved_sessions` most-recent-first) so the login screen is skipped
          // when the user was still logged in on close. A failed resume
          // (expired session) or a 2FA prompt falls back to `login` via the
          // reducer; an empty session list shows the login form directly.
          const last = b.saved_sessions[0];
          if (last) {
            if (!autoLoginRef.current) {
              autoLoginRef.current = true;
              loginSaved(last);
            }
          } else {
            setState((s) => ({ ...s, screen: "login" }));
          }
        })
        .catch((err) => {
          console.error(
            "bootstrap failed: saved sessions, reaction emojis, and theme were not loaded",
            err,
          );
          if (!disposed) setState((s) => ({ ...s, screen: "login" }));
        });
    })
      .catch((err) => {
        // If the event channel can't be established, don't hang the boot
        // splash — surface the login screen instead.
        console.error("event subscription failed:", err);
        if (!disposed) setState((s) => ({ ...s, screen: "login" }));
      });

    // Theme/chat-theme changes from the Settings window (broadcast by the backend).
    listen<string>("igdm://theme", (e) => {
      if (disposed || !isTheme(e.payload)) return;
      applyTheme(e.payload);
    }).then((u) => {
      if (disposed) u();
      else unlistenTheme = u;
    });
    listen<boolean>("igdm://chat-themes", (e) => {
      if (disposed) return;
      setState((s) => ({ ...s, chatThemesEnabled: e.payload }));
    }).then((u) => {
      if (disposed) u();
      else unlistenChatThemes = u;
    });

    return () => {
      disposed = true;
      unlistenEvents?.();
      unlistenTheme?.();
      unlistenChatThemes?.();
    };
  }, []);

  const setSearch = useCallback((query: string) => {
    setState((s) => ({
      ...s,
      searchQuery: query,
      showSearch: query.trim().length > 0,
      searching: query.trim().length > 0,
    }));
  }, []);

  const clearSearch = useCallback(() => {
    setState((s) => ({
      ...s,
      searchQuery: "",
      searchResults: [],
      showSearch: false,
      searching: false,
    }));
  }, []);

  const setReply = useCallback((reply: ReplyInfo | null) => {
    setState((s) => ({ ...s, reply }));
  }, []);

  const setReplyScroll = useCallback((msgId: string | null, retries: number) => {
    setState((s) => ({ ...s, replyScroll: msgId, replyRetries: retries }));
  }, []);

  const refreshInbox = useCallback(() => {
    setState((s) => ({ ...s, inboxLoading: true }));
    api.refreshInbox();
  }, []);

  // `busy` is set here and cleared only by the reducer (LoggedIn/LoginError/CodePrompt).
  const loginPassword = useCallback((username: string, password: string) => {
    setState((s) => ({ ...s, login: { ...s.login, busy: true, error: "" } }));
    api.loginPassword(username, password);
  }, []);

  const loginSessionid = useCallback((sessionid: string) => {
    setState((s) => ({ ...s, login: { ...s.login, busy: true, error: "" } }));
    api.loginSessionid(sessionid);
  }, []);

  const loginSaved = useCallback((name: string) => {
    setState((s) => ({
      ...s,
      login: { ...s.login, busy: true, error: "", pendingSession: name },
    }));
    api.loginSaved(name);
  }, []);

  const saveReactionEmojis = useCallback((emojis: string[]) => {
    api.saveReactionEmojis(emojis);
    setState((s) => ({ ...s, reactionEmojis: emojis }));
  }, []);

  const mutateThread = useCallback((key: string, fn: (ts: ThreadState) => ThreadState) => {
    setState((s) => {
      const threads = { ...s.threads };
      if (threads[key]) threads[key] = fn(threads[key]);
      return { ...s, threads };
    });
  }, []);

  const sendText = useCallback(
    (threadId: string, text: string, userIds: string[], replyTo: ReplyRef | null) => {
      const viewer = stateRef.current.me.user_id;
      const isOpen = stateRef.current.openKey === threadId;
      mutateThread(threadId, (ts) => {
        const replyEcho: DirectMessage | null = replyTo
          ? (ts.messages.find((m) => m.id === replyTo.message_id) ?? null)
          : null;
        const echo: DirectMessage = {
          id: `local:${crypto.randomUUID()}`,
          user_id: viewer,
          timestamp: new Date().toISOString(),
          item_type: "text",
          text,
          reply: replyEcho,
        };
        const messages = sortMessages({ ...ts, messages: [...ts.messages, echo] }).messages;
        return {
          ...ts,
          messages,
          last_activity: tsMillis(echo) / 1000,
        };
      });
      setState((s) => {
        if (!isOpen) return s;
        return { ...s, reply: s.reply && s.reply.threadKey === threadId ? null : s.reply };
      });
      api.sendText(threadId, text, userIds, replyTo);
    },
    [mutateThread],
  );

  const openThread = useCallback((key: string) => {
    setState((s) => {
      const threads = { ...s.threads };
      const ts = threads[key];
      if (ts) {
        threads[key] = { ...ts, unread: false };
      }
      return { ...s, threads, openKey: key };
    });
  }, []);

  const openPendingThread = useCallback((key: string) => {
    setState((s) => {
      const threads = { ...s.threads };
      if (!threads[key]) {
        threads[key] = { ...emptyThreadState(key), pending: true };
      }
      return { ...s, threads, openKey: key };
    });
  }, []);

  const value: AppContextValue = useMemo(
    () => ({
      state,
      loginPassword,
      loginSessionid,
      loginSaved,
      provideCode,
      cancelCode,
      logout,
      refreshInbox,
      loadMessages,
      loadOlder,
      threadDetails,
      threadRaw,
      approveRequest,
      searchUsers,
      threadForUser,
      saveReactionEmojis,
      openThread,
      openPendingThread,
      sendText,
      sendPhoto,
      sendPhotoBytes,
      sendVideo,
      sendVoice,
      sendReaction,
      markSeen,
      sendTyping,
      downloadMedia,
      setSearch,
      clearSearch,
      setReply,
      setReplyScroll,
      updateThread: mutateThread,
    }),
    [
      state,
      loginPassword,
      loginSessionid,
      loginSaved,
      refreshInbox,
      saveReactionEmojis,
      mutateThread,
      openThread,
      openPendingThread,
      sendText,
      setSearch,
      clearSearch,
      setReply,
      setReplyScroll,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}
