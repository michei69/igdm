import type { UIEvent } from "react";
import Avatar from "../common/Avatar";
import { isNumber } from "../../lib/guards";

export interface ShareComment {
  pk?: string;
  text?: string;
  created_at?: number;
  like_count?: number;
  user?: { username?: string; profile_pic_url?: string };
}

interface Props {
  comments: ShareComment[];
  commentCount: number;
  loading: boolean;
  loadingMore: boolean;
  onScroll: (e: UIEvent<HTMLDivElement>) => void;
}

export default function ShareCommentsPanel({
  comments,
  commentCount,
  loading,
  loadingMore,
  onScroll,
}: Props) {
  return (
    <div className="flex h-[55vh] w-[260px] shrink-0 flex-col border-l border-border pl-3">
      <span className="pb-1.5 text-[12px] font-semibold text-ink2">
        Comments{commentCount > 0 ? ` (${commentCount})` : ""}
      </span>
      <div className="min-h-0 flex-1 overflow-y-auto pr-1" onScroll={onScroll}>
        {loading ? (
          <span className="text-[12px] text-ink3">Loading comments…</span>
        ) : comments.length === 0 ? (
          <span className="text-[12px] text-ink3">No comments yet</span>
        ) : (
          comments.map((c) => (
            <div key={c.pk ?? c.text ?? String(c.created_at)} className="mb-2.5 flex gap-2">
              <Avatar
                name={c.user?.username ?? "?"}
                url={c.user?.profile_pic_url ?? null}
                size={28}
              />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="text-[12px] font-semibold text-ink">
                  {c.user?.username ?? "unknown"}
                </span>
                <span className="text-[12px] leading-snug whitespace-pre-wrap text-ink2">
                  {c.text}
                </span>
                <span className="text-[10px] text-ink3">
                  {isNumber(c.created_at)
                    ? new Date(c.created_at * 1000).toLocaleDateString(undefined, {
                        month: "short",
                        day: "numeric",
                      })
                    : ""}
                  {isNumber(c.like_count) ? ` · ♥ ${c.like_count.toLocaleString()}` : ""}
                </span>
              </div>
            </div>
          ))
        )}
        {loadingMore && (
          <span className="block py-1 text-center text-[11px] text-ink3">Loading more…</span>
        )}
      </div>
    </div>
  );
}
