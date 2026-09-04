import { useApp } from "../../hooks/useApp";

export default function ApproveBar() {
  const { state, approveRequest } = useApp();
  const openKey = state.openKey;
  const ts = openKey ? state.threads[openKey] : undefined;
  if (!ts || !ts.pending) return null;

  return (
    <div className="flex w-full shrink-0 items-center gap-3 border-b border-border bg-panel px-4 py-2">
      <span className="flex-1 text-[12px] text-ink2">This is a message request.</span>
      <button className="btn btn-primary btn-sm" onClick={() => approveRequest(ts.key)}>
        Accept
      </button>
    </div>
  );
}
