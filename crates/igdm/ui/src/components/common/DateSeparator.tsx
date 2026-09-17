import { formatDate } from "../../lib/format";

export default function DateSeparator({ when }: { when: Date }) {
  return (
    <div className="flex w-full justify-center px-4 pt-3 pb-2">
      <span
        className="select-none text-[11px] font-semibold"
        style={{ color: "var(--ct-secondary)" }}
      >
        {formatDate(when)}
      </span>
    </div>
  );
}
