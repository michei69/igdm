import { memo } from "react";
import type { ThreadView } from "../../lib/threadView";
import Avatar from "../common/Avatar";

interface Props {
  view: ThreadView;
  selected: boolean;
  onPress: (key: string) => void;
  onSecondary?: (key: string, x: number, y: number) => void;
}

function ThreadRow({ view, selected, onPress, onSecondary }: Props) {
  return (
    <button
      type="button"
      className={`flex h-16 w-full items-center gap-2.5 rounded-[10px] px-1.5 py-2.5 text-left ${
        selected ? "bg-panel2" : "bg-panel"
      }`}
      onClick={() => onPress(view.key)}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onSecondary?.(view.key, e.clientX, e.clientY);
      }}
    >
      <Avatar name={view.avatarName} url={view.avatarUrl} size={48} />
      <div className="flex min-w-0 flex-1 flex-col gap-[3px]">
        <div className="flex w-full items-center justify-between gap-2">
          <span className="truncate text-[13.5px] font-semibold text-ink">{view.title}</span>
          <span className="shrink-0 select-none text-[11px] text-ink3">{view.time}</span>
        </div>
        <div className="flex w-full items-center justify-between gap-2">
          <span className={`truncate text-[12px] ${view.unread ? "text-ink" : "text-ink2"}`}>
            {view.preview}
          </span>
          {view.unread && <span className="h-2 w-2 shrink-0 rounded-full bg-accent" />}
        </div>
      </div>
    </button>
  );
}

export default memo(ThreadRow);
