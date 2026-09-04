import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useVirtualizer, type ReactVirtualizer } from "@tanstack/react-virtual";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { toast } from "sonner";
import { useDebouncedCallback } from "use-debounce";
import { useApp } from "../../hooks/useApp";
import { threadTitle, titleForUser, type ThreadState, type UserShort } from "../../state";
import Avatar from "../common/Avatar";
import ContextMenu, { MenuItem } from "../common/ContextMenu";
import ThreadRow from "./ThreadRow";
import { lastMessageFrom, threadViewFromState, type ThreadView } from "../../lib/threadView";
import { copyClipboard } from "../../lib/clipboard";
import { api } from "../../lib/api";

const ROW_HEIGHT = 66;

/** Whether a thread appears in the sidebar search results for `query`. */
function matchesThreadQuery(t: ThreadState, query: string): boolean {
  if (t.key.startsWith("user:") && t.messages.length === 0) return false;
  if (query.length === 0) return true;
  const title = threadTitle(t).toLowerCase();
  const uname = (t.users[0]?.username ?? "").toLowerCase();
  return title.includes(query) || uname.includes(query);
}

interface StatusLine {
  text: string;
  className: string;
}

/** Connection status line: connected, a detail string, or "connecting". */
function statusLineFor(connected: boolean, detail: string): StatusLine {
  if (connected) return { text: "live · MQTT connected", className: "text-live" };
  if (detail.length > 0) return { text: detail.slice(0, 40), className: "text-ink3" };
  return { text: "connecting…", className: "text-ink3" };
}

function SearchResults({
  searching,
  results,
  onPress,
}: {
  searching: boolean;
  results: UserShort[];
  onPress: (key: string) => void;
}) {
  if (searching) return <div className="px-3 py-2 text-[12px] text-ink3">Searching…</div>;
  if (results.length === 0)
    return <div className="px-3 py-2 text-[12px] text-ink3">No people found</div>;
  return (
    <div className="flex h-full flex-col gap-0.5 overflow-y-auto">
      {results.slice(0, 15).map((u) => (
        <ThreadRow
          key={u.pk}
          view={{
            key: `user:${u.pk}`,
            title: titleForUser(u),
            preview: "",
            time: "",
            avatarUrl: u.profile_pic_url || null,
            avatarName: titleForUser(u),
            unread: false,
          }}
          selected={false}
          onPress={onPress}
        />
      ))}
    </div>
  );
}

function ThreadList({
  parentRef,
  virtualizer,
  threadViews,
  showSkeleton,
  onPress,
  onSecondary,
}: {
  parentRef: RefObject<HTMLDivElement | null>;
  virtualizer: ReactVirtualizer<HTMLDivElement, Element>;
  threadViews: { key: string; view: ThreadView; selected: boolean }[];
  showSkeleton: boolean;
  onPress: (key: string) => void;
  onSecondary: (key: string, x: number, y: number) => void;
}) {
  return (
    <div ref={parentRef} className="h-full overflow-y-auto">
      {showSkeleton ? (
        <div className="flex flex-col gap-1.5 p-1" aria-hidden="true">
          {Array.from({ length: 8 }, (_, i) => (
            <div key={i} className="flex h-16 items-center gap-2.5 px-1.5">
              <div className="skeleton h-12 w-12 shrink-0 rounded-full" />
              <div className="flex min-w-0 flex-1 flex-col gap-2">
                <div className="skeleton h-3.5 w-2/3 rounded" />
                <div className="skeleton h-3 w-1/2 rounded" />
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const threadView = threadViews[item.index];
            return (
              <div
                key={threadView.key}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <ThreadRow
                  view={threadView.view}
                  selected={threadView.selected}
                  onPress={onPress}
                  onSecondary={onSecondary}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default function Sidebar() {
  const {
    state,
    openThread,
    markSeen,
    loadMessages,
    threadDetails,
    updateThread,
    searchUsers,
    setSearch,
    clearSearch,
    threadForUser,
    refreshInbox,
    logout,
    threadRaw,
  } = useApp();
  const me = state.me;
  const openKey = state.openKey;

  const [mainMenu, setMainMenu] = useState<{ x: number; y: number } | null>(null);
  const [threadMenu, setThreadMenu] = useState<{ key: string; x: number; y: number } | null>(null);

  // 350ms debounce on the search query.
  const debouncedSearch = useDebouncedCallback((q: string) => searchUsers(q), 350);
  useEffect(() => {
    const q = state.searchQuery.trim();
    if (q.length === 0) return;
    debouncedSearch(q);
    return () => debouncedSearch.cancel();
  }, [state.searchQuery, debouncedSearch]);

  const threads = useMemo(() => {
    const q = state.searchQuery.trim().toLowerCase();
    return Object.values(state.threads)
      .filter((t) => matchesThreadQuery(t, q))
      .toSorted((a, b) => b.last_activity - a.last_activity);
  }, [state.threads, state.searchQuery]);

  const parentRef = useRef<HTMLDivElement | null>(null);
  const virtualizer = useVirtualizer({
    count: threads.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
    // Same as MessageList: avoid flushSync during commit-phase measurements.
    useFlushSync: false,
  });

  const onThreadClick = useCallback(
    (threadKey: string) => {
      const ts = state.threads[threadKey];
      if (!ts) return;
      openThread(ts.key);
      // Reaction echoes are not messages: their ids are reaction-item ids the
      // server rejects for read receipts (HTTP 500).
      const last = lastMessageFrom(ts);
      if (last && last.user_id && last.user_id !== me.user_id) {
        markSeen(ts.key, last.id, last.raw ?? undefined);
      }
      if (!ts.oldest_cursor && !ts.key.startsWith("user:")) {
        loadMessages(ts.key, 30);
      }
      const needsMeta =
        Object.keys(ts.nicknames).length === 0 || (ts.is_group && ts.avatar.length === 0);
      if (needsMeta && !ts.meta_fetching) {
        updateThread(ts.key, (t) => ({ ...t, meta_fetching: true }));
        threadDetails(ts.key);
      }
    },
    [state.threads, me.user_id, openThread, markSeen, loadMessages, updateThread, threadDetails],
  );

  const onThreadSecondary = useCallback((threadKey: string, x: number, y: number) => {
    setThreadMenu({ key: threadKey, x, y });
  }, []);

  const threadViews = useMemo(() => {
    return threads.map((ts) => ({
      key: ts.key,
      view: threadViewFromState(ts),
      selected: ts.key === openKey,
    }));
  }, [threads, openKey]);

  const onSearchClick = useCallback(
    (key: string) => {
      const pk = key.slice("user:".length);
      const user = state.searchResults.find((u) => String(u.pk) === pk);
      if (!user) return;
      clearSearch();
      threadForUser(user);
    },
    [state.searchResults, clearSearch, threadForUser],
  );

  const openSettings = () => {
    setMainMenu(null);
    void WebviewWindow.getByLabel("settings").then((existing) => {
      if (existing) {
        existing.show();
        void existing.setFocus();
        return;
      }
      void new WebviewWindow("settings", {
        url: "/settings",
        title: "Settings",
        width: 520,
        height: 560,
        center: true,
        focus: true,
      });
    });
  };

  const copyThreadRaw = (key: string) => {
    setThreadMenu(null);
    const ts = state.threads[key];
    // Copies the raw API response body when available; falls back to the
    // client-side thread state only when the server has nothing (a brand-new
    // `user:` chat) so the action never copies nothing.
    void threadRaw(key).then((raw) => {
      const data = raw ?? ts;
      if (!data) {
        toast("No data for this chat yet");
        return;
      }
      const text = JSON.stringify(data, null, 2);
      // GTK clipboard first: the response body can be hundreds of KB and the
      // plugin's Wayland backend fails to serve large offers (paste is empty
      // despite a successful write). Fall back to the plugin if unavailable.
      void api
        .copyLargeText(text)
        .then(() => {
          toast(raw ? "Thread raw data copied" : "Thread data copied");
        })
        .catch(async () => {
          const ok = await copyClipboard(text);
          toast(
            ok
              ? raw
                ? "Thread raw data copied"
                : "Thread data copied"
              : "Copy failed — clipboard unavailable",
          );
        });
    });
  };

  const statusLine = statusLineFor(state.connected, state.statusDetail);

  // Skeleton from startup until real chats arrive; only a finished fetch with
  // zero chats ("genuinely empty" inbox) suppresses it, hence `!state.connected`.
  const showSkeleton = threads.length === 0 && (state.inboxLoading || !state.connected);

  return (
    <div className="flex h-full w-full flex-col gap-2 bg-panel p-3">
      {/* Header */}
      <div className="flex items-center gap-2.5">
        <Avatar name={me.username} url={me.profile_pic_url || null} size={40} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13.5px] font-bold text-ink">{me.username}</div>
          <div className={`truncate text-[11px] ${statusLine.className}`}>{statusLine.text}</div>
        </div>
        <button
          className="shrink-0 rounded-md px-1.5 text-[18px] leading-none text-ink2 hover:bg-panel2"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setMainMenu({ x: r.right - 160, y: r.bottom + 4 });
          }}
          aria-label="Main menu"
        >
          ⋯
        </button>
      </div>

      {/* Search */}
      <input
        className="w-full rounded-lg border border-border bg-panel2 px-3 py-1.5 text-[13px] text-ink outline-none focus:border-accent"
        placeholder="Search chats"
        aria-label="Search chats"
        value={state.searchQuery}
        onChange={(e) => setSearch(e.target.value)}
      />

      {/* Search results or thread list */}
      <div className="relative min-h-0 flex-1">
        {state.showSearch ? (
          <SearchResults
            searching={state.searching}
            results={state.searchResults}
            onPress={onSearchClick}
          />
        ) : (
          <ThreadList
            parentRef={parentRef}
            virtualizer={virtualizer}
            threadViews={threadViews}
            showSkeleton={showSkeleton}
            onPress={onThreadClick}
            onSecondary={onThreadSecondary}
          />
        )}
      </div>

      {mainMenu && (
        <ContextMenu x={mainMenu.x} y={mainMenu.y} onClose={() => setMainMenu(null)}>
          <MenuItem
            label="Refresh inbox"
            onSelect={() => {
              setMainMenu(null);
              refreshInbox();
            }}
          />
          <MenuItem label="Settings" onSelect={openSettings} />
          <MenuItem
            label="Log out"
            danger
            onSelect={() => {
              setMainMenu(null);
              logout();
            }}
          />
        </ContextMenu>
      )}

      {threadMenu && (
        <ContextMenu x={threadMenu.x} y={threadMenu.y} onClose={() => setThreadMenu(null)}>
          <MenuItem label="Copy raw data" onSelect={() => copyThreadRaw(threadMenu.key)} />
        </ContextMenu>
      )}
    </div>
  );
}
