// Tauri event subscription for the `igdm://event` channel. The Rust side
// serializes `AppEvent` as `{"type": ..., "data": ...}` with tuple variants
// as JSON arrays; decode that into a named-field `AppEvent` here so the rest
// of the app never sees wire tuples. Each `data as [...]` below documents the
// exact tuple the Rust `AppEvent` variant serializes.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppEvent,
  DirectMessage,
  DirectThread,
  LiveMessage,
  MeInfo,
  ThreadMeta,
  UserShort,
} from "../state";

const EVENT_CHANNEL = "igdm://event";

interface WireEvent {
  type: string;
  data?: unknown;
}

export function decodeEvent(wire: WireEvent): AppEvent {
  switch (wire.type) {
    case "Status": {
      // SAFETY: Status serializes as [connected, detail].
      const [connected, detail] = wire.data as [boolean, string];
      return { type: "Status", connected, detail };
    }
    case "LoginError": {
      // SAFETY: LoginError serializes as [text].
      const [text] = wire.data as [string];
      return { type: "LoginError", text };
    }
    case "LoggedIn": {
      // SAFETY: LoggedIn serializes as [me].
      const [me] = wire.data as [MeInfo];
      return { type: "LoggedIn", me };
    }
    case "LoggedOut":
      return { type: "LoggedOut" };
    case "CodePrompt": {
      // SAFETY: CodePrompt serializes as [text].
      const [text] = wire.data as [string];
      return { type: "CodePrompt", text };
    }
    case "LiveMessage": {
      // SAFETY: LiveMessage serializes as [live].
      const [live] = wire.data as [LiveMessage];
      return { type: "LiveMessage", live };
    }
    case "Typing": {
      // SAFETY: Typing serializes as [threadId, senderId, active].
      const [threadId, senderId, active] = wire.data as [string, string, boolean];
      return { type: "Typing", threadId, senderId, active };
    }
    case "Seen": {
      // SAFETY: Seen serializes as [threadId, userId, itemId].
      const [threadId, userId, itemId] = wire.data as [string, string, string];
      return { type: "Seen", threadId, userId, itemId };
    }
    case "ThreadsLoaded": {
      // SAFETY: ThreadsLoaded serializes as [threads, meta].
      const [threads, meta] = wire.data as [DirectThread[], Record<string, ThreadMeta>];
      return { type: "ThreadsLoaded", threads, meta };
    }
    case "ThreadDetails": {
      // SAFETY: ThreadDetails serializes as [threadId, thread, meta].
      const [threadId, thread, meta] = wire.data as [string, DirectThread, ThreadMeta];
      return { type: "ThreadDetails", threadId, thread, meta };
    }
    case "MessagesLoaded": {
      // SAFETY: MessagesLoaded serializes as [threadId, messages, cursor, hasMore].
      const [threadId, messages, cursor, hasMore] = wire.data as [
        string,
        DirectMessage[],
        string | null,
        boolean,
      ];
      return { type: "MessagesLoaded", threadId, messages, cursor, hasMore };
    }
    case "OlderLoaded": {
      // SAFETY: OlderLoaded serializes as [threadId, messages, cursor, hasMore].
      const [threadId, messages, cursor, hasMore] = wire.data as [
        string,
        DirectMessage[],
        string | null,
        boolean,
      ];
      return { type: "OlderLoaded", threadId, messages, cursor, hasMore };
    }
    case "Sent": {
      // SAFETY: Sent serializes as { key, real_thread_id, msg }.
      const {
        key,
        real_thread_id: realThreadId,
        msg,
      } = wire.data as {
        key: string;
        real_thread_id: string;
        msg: DirectMessage;
      };
      return { type: "Sent", key, realThreadId, msg };
    }
    case "SendFailed": {
      // SAFETY: SendFailed serializes as [key, text].
      const [key, text] = wire.data as [string, string];
      return { type: "SendFailed", key, text };
    }
    case "SearchResults": {
      // SAFETY: SearchResults serializes as [query, users].
      const [query, users] = wire.data as [string, UserShort[]];
      return { type: "SearchResults", query, users };
    }
    case "SearchFailed": {
      // SAFETY: SearchFailed serializes as [query].
      const [query] = wire.data as [string];
      return { type: "SearchFailed", query };
    }
    case "ThreadByUser": {
      // SAFETY: ThreadByUser serializes as [user, threadId].
      const [user, threadId] = wire.data as [UserShort, string | null];
      return { type: "ThreadByUser", user, threadId };
    }
    case "Approved": {
      // SAFETY: Approved serializes as [key].
      const [key] = wire.data as [string];
      return { type: "Approved", key };
    }
    case "MediaDone": {
      // SAFETY: MediaDone serializes as [path].
      const [path] = wire.data as [string];
      return { type: "MediaDone", path };
    }
    case "MediaFailed": {
      // SAFETY: MediaFailed serializes as [text].
      const [text] = wire.data as [string];
      return { type: "MediaFailed", text };
    }
    default:
      throw new Error(`Unknown backend event type: ${wire.type}`);
  }
}

/** Subscribe to backend events. Returns an unlisten function. */
export async function subscribeEvents(handler: (event: AppEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<WireEvent>(EVENT_CHANNEL, (e) => {
    handler(decodeEvent(e.payload));
  });
  return unlisten;
}
