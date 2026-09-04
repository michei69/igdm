// Pure reducer for backend events. Side effects are returned separately so
// the caller (AppProvider) can run them after committing state.

import type {
  AppEvent,
  AppState,
  DirectMessage,
  DirectThread,
  ThreadMeta,
  ThreadState,
} from "../state";
import {
  DEFAULT_LOGIN_STATE,
  displayName,
  emptyThreadState,
  sortMessages,
  threadTitleFrom,
  titleForUser,
  tsMillis,
  unreadFor,
} from "../state";
import { messagePreview } from "./format";
import { type Json, type JsonObject } from "./guards";
import {
  applyReaction,
  isReactionItem,
  matchesMessage,
  reactionFromItem,
  removeReaction,
  type IncomingReaction,
} from "./reactions";

export type Effect =
  | { kind: "refresh_inbox" }
  | { kind: "thread_details"; threadId: string }
  | { kind: "mark_seen"; threadId: string; itemId: string; raw?: Json }
  | { kind: "load_older"; threadId: string; cursor: string }
  | { kind: "reveal_media"; path: string }
  | { kind: "toast"; text: string }
  | { kind: "notify"; title: string; body: string };

interface ReducerResult {
  state: AppState;
  effects: Effect[];
}

function withThread(state: AppState, key: string, fn: (ts: ThreadState) => ThreadState): AppState {
  const threads = { ...state.threads };
  threads[key] = fn(threads[key]);
  return { ...state, threads };
}

/**
 * Merge a server message into a thread's list. Server echoes of a send must
 * replace the matching `local:` echo (matched by text); known ids replace in
 * place; anything else appends. With `keepReply`, the UI-built reply quote is
 * preserved because server echoes of replies drop the replied-to payload.
 */
function mergeIncoming(
  messages: DirectMessage[],
  msg: DirectMessage,
  opts: { localText?: string | null; keepReply?: boolean },
): DirectMessage[] {
  const known = messages.some((m) => m.id === msg.id);
  if (known) {
    return messages.map((m) => {
      if (m.id !== msg.id) return m;
      let next = msg;
      // A condensed echo (e.g. a media send bounced back without the `media`
      // object) must not clobber a row that already carries the full media
      // payload — keep the richer one.
      if (!msg.media && m.media) {
        const mRaw = m.raw;
        // SAFETY: the condensed echo keeps the richer media from the existing
        // row only when its raw payload still carries a media object.
        const raw = (mRaw as JsonObject | null)?.media ? m.raw : msg.raw;
        next = { ...msg, media: m.media, raw };
      }
      if (opts.keepReply && !next.reply && m.reply) return { ...next, reply: m.reply };
      return next;
    });
  }
  const idx = opts.localText
    ? messages.findIndex((m) => m.id.startsWith("local:") && m.text === opts.localText)
    : -1;
  if (idx !== -1) {
    const next = [...messages];
    const echo = next[idx];
    next[idx] = opts.keepReply && !msg.reply && echo.reply ? { ...msg, reply: echo.reply } : msg;
    return next;
  }
  return [...messages, msg];
}

/** Merge a DirectThread into local state. */
function upsertThread(state: AppState, thread: DirectThread, meta?: ThreadMeta): AppState {
  const key = thread.id;
  const existing = state.threads[key] ?? emptyThreadState(key);
  let ts: ThreadState = {
    ...existing,
    title: threadTitleFrom(thread),
    users: thread.users,
    is_group: thread.is_group,
    pending: thread.pending,
    last_activity: new Date(thread.last_activity_at).getTime() / 1000,
    read_state: thread.read_state ?? 0,
    theme_data: thread.theme_data ?? null,
    last_seen_at: Object.fromEntries(
      Object.entries(thread.last_seen_at).map(([uid, info]) => [
        uid,
        info.timestamp ? new Date(info.timestamp).getTime() / 1000 : 0,
      ]),
    ),
  };
  if (meta) {
    ts = { ...ts, nicknames: meta.nicknames, avatar: meta.avatar };
  }
  if (!existing.loaded) {
    ts = sortMessages({ ...ts, messages: thread.messages, loaded: true });
  }
  ts = { ...ts, unread: unreadFor(ts, state.me.user_id) };
  return withThread(state, key, () => ts);
}

export function applyEvent(state: AppState, event: AppEvent): ReducerResult {
  const effects: Effect[] = [];
  switch (event.type) {
    case "Status": {
      const next: AppState = { ...state, connected: event.connected, statusDetail: event.detail };
      if (event.connected && Object.keys(next.threads).length === 0) {
        effects.push({ kind: "refresh_inbox" });
        return { state: { ...next, inboxLoading: true }, effects };
      }
      return { state: next, effects };
    }

    case "LoginError":
      return {
        state: {
          ...state,
          // A failed resume attempt falls back to the login screen (which
          // still lists the saved sessions) instead of a stuck boot splash.
          screen: "login",
          login: { ...state.login, busy: false, pendingSession: null, error: event.text },
        },
        effects,
      };

    case "LoggedIn":
      return {
        state: {
          ...state,
          me: event.me,
          screen: "main",
          login: { ...DEFAULT_LOGIN_STATE },
        },
        effects,
      };

    case "LoggedOut":
      return {
        state: {
          ...state,
          screen: "login",
          threads: {},
          threadRaw: {},
          openKey: null,
          connected: false,
          statusDetail: "",
          reply: null,
          replyScroll: null,
          inboxLoading: false,
          chatThemesEnabled: true,
        },
        effects,
      };

    case "CodePrompt":
      return {
        state: {
          ...state,
          // A resume hit two-factor auth: the code prompt renders on the
          // login screen, so leave the boot splash.
          screen: "login",
          login: {
            ...state.login,
            busy: false,
            error: "",
            pendingSession: null,
            code_info: event.text,
            show_code: true,
          },
        },
        effects,
      };

    case "LiveMessage": {
      const live = event.live;
      const liveMsg = live.message;
      let next = state;
      if (!next.threads[live.thread_id]) {
        next = withThread(next, live.thread_id, () => ({
          ...emptyThreadState(live.thread_id),
          title: "New conversation",
        }));
        effects.push({ kind: "thread_details", threadId: live.thread_id });
      }
      const own = live.user_id === next.me.user_id;
      // Reaction items (like/unlike) are not messages: their ids are
      // reaction-item ids the server rejects for read receipts, and they
      // must attach to (or detach from) the target message instead of
      // becoming rows. Own reaction echoes carry no user id — the viewer
      // reacted, so attribute them to me.
      const liveReaction = live.message ? reactionFromItem(live.message) : null;
      const reactionItem = live.message ? isReactionItem(live.message) : false;
      // System notification: only genuinely new incoming messages (not own
      // sends, reactions, edits/removes, system rows, or server resends of
      // known ids). The effect runner shows it only while the window is
      // unfocused.
      const knownBefore = liveMsg
        ? next.threads[live.thread_id].messages.some((m) => m.id === liveMsg.id)
        : true;
      const notifyNew =
        live.op !== "remove" &&
        !own &&
        !!liveMsg &&
        !liveReaction &&
        !reactionItem &&
        !liveMsg.action_log &&
        liveMsg.item_type !== "video_call_event" &&
        liveMsg.item_type !== "audio_call_event" &&
        !knownBefore;
      next = withThread(next, live.thread_id, (ts) => {
        let t = ts;
        if (live.op === "remove") {
          if (liveReaction) {
            const senderId = liveReaction.senderId || next.me.user_id;
            t = {
              ...t,
              messages: removeReaction(t.messages, { ...liveReaction, senderId }),
            };
          } else {
            t = { ...t, messages: t.messages.filter((m) => m.id !== live.item_id) };
          }
        } else if (live.message) {
          const msg = live.message;
          if (liveReaction) {
            // Reaction patches never become message rows; they attach to the
            // target message (and don't bump the unread badge).
            const hasTarget = t.messages.some((m) => matchesMessage(m, liveReaction.messageId));
            if (hasTarget) {
              const senderId = liveReaction.senderId || next.me.user_id;
              t = {
                ...t,
                messages: applyReaction(t.messages, { ...liveReaction, senderId }),
                last_activity: tsMillis(msg) / 1000,
              };
            }
          } else if (reactionItem) {
            // Reaction-shaped item with nothing actionable (e.g. a cleared
            // reactions list): never a message row, never a read receipt.
          } else {
            const knownInThread = t.messages.some((m) => m.id === msg.id);
            t = sortMessages({
              ...t,
              messages: mergeIncoming(t.messages, msg, { localText: own ? live.text : null }),
              last_activity: tsMillis(msg) / 1000,
            });
            if (!own && !knownInThread) {
              t = { ...t, unread: true };
            }
          }
        }
        return t;
      });
      if (notifyNew && live.message) {
        const ts = next.threads[live.thread_id];
        const user = ts.users.find((u) => u.pk === live.user_id);
        const sender = user ? displayName(ts, user) : live.user_id || "Someone";
        const rawPreview = live.message.text?.trim() || messagePreview(live.message) || "";
        const preview =
          rawPreview.startsWith("<") && rawPreview.endsWith(">") ? "Message" : rawPreview;
        effects.push({
          kind: "notify",
          title: ts.title && ts.title.length > 0 ? ts.title : "IG Direct",
          body: ts.is_group ? `${sender}: ${preview}` : preview,
        });
      }
      if (next.openKey === live.thread_id) {
        next = withThread(next, live.thread_id, (t) => ({ ...t, unread: false }));
        // Only real message rows get read receipts: reaction items and
        // removes carry non-message ids the server rejects (HTTP 500).
        if (live.message && live.op !== "remove" && !liveReaction && !reactionItem) {
          effects.push({
            kind: "mark_seen",
            threadId: live.thread_id,
            itemId: live.item_id,
            // Raw payload of the message being marked seen, so the backend can
            // log it if the read receipt fails.
            raw: live.message.raw ?? null,
          });
        }
      }
      return { state: next, effects };
    }

    case "Typing": {
      const { threadId, senderId, active } = event;
      if (!state.threads[threadId]) return { state, effects };
      const expiry = Math.floor(Date.now() / 1000) + 6;
      return {
        state: withThread(state, threadId, (ts) => {
          const typing = { ...ts.typing };
          if (active) {
            typing[senderId] = expiry;
          } else {
            delete typing[senderId];
          }
          return { ...ts, typing };
        }),
        effects,
      };
    }

    case "Seen": {
      const { threadId, userId, itemId } = event;
      if (!state.threads[threadId] || itemId.length === 0) return { state, effects };
      return {
        state: withThread(state, threadId, (ts) => ({
          ...ts,
          seen_by: { ...ts.seen_by, [userId]: itemId },
          // The read receipt just arrived: the sender has seen up to now.
          last_seen_at: { ...ts.last_seen_at, [userId]: Math.floor(Date.now() / 1000) },
        })),
        effects,
      };
    }

    case "ThreadsLoaded": {
      let next = state;
      for (const thread of event.threads) {
        next = upsertThread(next, thread, event.meta[thread.id]);
      }
      return { state: { ...next, inboxLoading: false }, effects };
    }

    case "ThreadDetails":
      return { state: upsertThread(state, event.thread, event.meta), effects };

    case "MessagesLoaded": {
      const { threadId, messages, cursor, hasMore } = event;
      if (!state.threads[threadId]) return { state, effects };
      const fetched = new Set(messages.map((m) => m.id));
      const live = state.threads[threadId].messages.filter((m) => !fetched.has(m.id));
      const items: DirectMessage[] = [];
      const reactions: IncomingReaction[] = [];
      for (const m of messages) {
        const reaction = reactionFromItem(m);
        if (reaction) reactions.push(reaction);
        else items.push(m);
      }
      let all = [...items, ...live].toSorted((a, b) => tsMillis(a) - tsMillis(b));
      for (const r of reactions) all = applyReaction(all, r);
      return {
        state: withThread(state, threadId, (ts) => ({
          ...ts,
          messages: all,
          oldest_cursor: cursor,
          has_more: hasMore && cursor !== null,
          loaded: true,
        })),
        effects,
      };
    }

    case "OlderLoaded": {
      const { threadId, messages, cursor, hasMore } = event;
      if (!state.threads[threadId]) return { state, effects };
      const known = new Set(state.threads[threadId].messages.map((m) => m.id));
      const older: DirectMessage[] = [];
      const reactions: IncomingReaction[] = [];
      for (const m of messages) {
        if (known.has(m.id)) continue;
        const reaction = reactionFromItem(m);
        if (reaction) reactions.push(reaction);
        else older.push(m);
      }
      let next = withThread(state, threadId, (ts) =>
        sortMessages({
          ...ts,
          messages: [...ts.messages, ...older],
          oldest_cursor: cursor,
          has_more: hasMore && cursor !== null,
          loading_older: false,
        }),
      );
      for (const r of reactions) {
        next = withThread(next, threadId, (ts) => ({
          ...ts,
          messages: applyReaction(ts.messages, r),
        }));
      }
      // Reply-target hunt: keep loading history until the replied-to message
      // shows up (up to the configured number of attempts).
      const target = next.replyScroll;
      if (target) {
        const found = next.threads[threadId].messages.some((m) => m.id === target);
        if (!found && next.replyRetries > 0) {
          const nextCursor = next.threads[threadId].oldest_cursor;
          if (nextCursor) {
            next = withThread(next, threadId, (ts) => ({ ...ts, loading_older: true }));
            next = { ...next, replyRetries: next.replyRetries - 1 };
            effects.push({ kind: "load_older", threadId, cursor: nextCursor });
          }
        }
        if (!found && next.replyRetries === 0) {
          next = {
            ...next,
            replyScroll: null,
            replyRetries: 0,
          };
          effects.push({ kind: "toast", text: "Couldn't find the replied-to message in history" });
        }
      }
      return { state: next, effects };
    }

    case "Sent": {
      const { key, realThreadId, msg } = event;
      let next = state;
      if (key.startsWith("user:")) {
        const virtual = next.threads[key];
        if (virtual) {
          const threads = { ...next.threads };
          const existingReal = threads[realThreadId] ?? emptyThreadState(realThreadId);
          const promoted: ThreadState = {
            ...existingReal,
            title: existingReal.title || virtual.title,
            users: existingReal.users.length > 0 ? existingReal.users : virtual.users,
            messages: [msg],
            loaded: true,
            last_activity: tsMillis(msg) / 1000,
          };
          delete threads[key];
          threads[realThreadId] = promoted;
          next = { ...next, threads };
        }
        next = { ...next, openKey: realThreadId };
        effects.push({ kind: "refresh_inbox" });
        return { state: { ...next, inboxLoading: true }, effects };
      }
      if (!next.threads[key]) return { state, effects };
      next = withThread(next, key, (ts) =>
        sortMessages({
          ...ts,
          messages: mergeIncoming(ts.messages, msg, { localText: msg.text, keepReply: true }),
          last_activity: tsMillis(msg) / 1000,
        }),
      );
      return { state: next, effects };
    }

    case "SendFailed": {
      if (!state.threads[event.key]) return { state, effects };
      const next = withThread(state, event.key, (ts) => ({
        ...ts,
        messages: ts.messages.filter((m) => !m.id.startsWith("local:")),
      }));
      effects.push({ kind: "toast", text: `Send failed: ${event.text}` });
      return { state: next, effects };
    }

    case "SearchResults": {
      if (state.searchQuery !== event.query) return { state, effects };
      return {
        state: { ...state, searchResults: event.users, searching: false },
        effects,
      };
    }

    case "SearchFailed":
      return { state: { ...state, searching: false }, effects };

    case "ThreadByUser": {
      const { user, threadId } = event;
      let next = state;
      if (threadId) {
        if (!next.threads[threadId]) {
          effects.push({ kind: "refresh_inbox" });
          next = withThread(next, threadId, () => ({
            ...emptyThreadState(threadId),
            title: titleForUser(user),
            users: [user],
            loaded: false,
          }));
          next = { ...next, inboxLoading: true };
        }
        next = { ...next, openKey: threadId };
      } else {
        const key = `user:${user.pk}`;
        next = withThread(next, key, () => ({
          ...emptyThreadState(key),
          title: titleForUser(user),
          users: [user],
          loaded: true,
        }));
        next = { ...next, openKey: key };
      }
      return { state: next, effects };
    }

    case "Approved": {
      let next = state;
      if (next.threads[event.key]) {
        next = withThread(next, event.key, (ts) => ({ ...ts, pending: false }));
      }
      effects.push({ kind: "toast", text: "Request accepted" });
      return { state: next, effects };
    }

    case "MediaDone":
      effects.push({ kind: "reveal_media", path: event.path });
      return { state, effects };

    case "MediaFailed":
      effects.push({ kind: "toast", text: `Download failed: ${event.text}` });
      return { state, effects };
  }
}
