// IG-style bottom sheet asking how to delete a message. Own messages offer
// "Delete for everyone" + "Delete for me"; incoming ones only "Delete for me".
// Rendering/exit is driven by motion; the caller keeps the target message in
// state until the sheet closes.

import { useEffect, useRef } from "react";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import type { DirectMessage } from "../../state";

/** ~240 ms of spring at the default motion stiffness. */
const SHEET_SPRING = { type: "spring", stiffness: 380, damping: 34, mass: 0.8 } as const;

interface Props {
  /** The message being deleted, or null when the sheet is closed. */
  msg: DirectMessage | null;
  /** True when the viewer sent the message (offers delete-for-everyone). */
  own: boolean;
  /** Waiting for the backend confirmation: actions disabled + spinner. */
  pending: boolean;
  onConfirm: (forSelf: boolean) => void;
  onClose: () => void;
}

export default function DeleteSheet({ msg, own, pending, onConfirm, onClose }: Props) {
  const reduce = useReducedMotion();
  const actionsRef = useRef<HTMLDivElement | null>(null);

  // Move focus into the sheet when it opens so Escape/Enter work without a
  // mouse; the pending state keeps focus on the disabled action.
  useEffect(() => {
    if (!msg) return;
    const t = window.setTimeout(() => {
      actionsRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    }, 40);
    return () => window.clearTimeout(t);
  }, [msg?.id]);

  return (
    <AnimatePresence>
      {msg && (
        <m.div
          key="delete-sheet"
          className="fixed inset-0 z-50 flex flex-col justify-end"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: reduce ? 0 : 0.18 }}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
          }}
        >
          <button
            type="button"
            className="absolute inset-0 bg-black/55"
            aria-label="Cancel delete"
            onClick={onClose}
          />
          <m.div
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-sheet-title"
            className="relative mx-auto w-full max-w-[440px] rounded-t-[20px] border-t border-border bg-panel px-4 pb-3.5 shadow-2xl outline-none"
            initial={reduce ? { opacity: 0 } : { y: "100%" }}
            animate={reduce ? { opacity: 1 } : { y: 0 }}
            exit={reduce ? { opacity: 0 } : { y: "100%" }}
            transition={reduce ? { duration: 0 } : SHEET_SPRING}
          >
            <div className="mx-auto mt-2.5 h-1 w-9 rounded-full bg-border" aria-hidden="true" />
            <h2
              id="delete-sheet-title"
              className="px-1 pt-3 pb-0.5 text-center text-[15px] font-semibold text-ink"
            >
              Delete message?
            </h2>
            <p className="pb-3 text-center text-[12px] text-ink3">This can’t be undone.</p>
            <div
              ref={actionsRef}
              className="flex flex-col overflow-hidden rounded-xl border border-border-soft"
            >
              {own && (
                <button
                  type="button"
                  className="delete-action text-danger"
                  disabled={pending}
                  onClick={() => onConfirm(false)}
                >
                  Delete for everyone
                </button>
              )}
              <button
                type="button"
                className={`delete-action ${own ? "border-t border-border-soft" : ""} text-ink`}
                disabled={pending}
                onClick={() => onConfirm(true)}
              >
                Delete for me
              </button>
            </div>
            {pending && (
              <div
                className="flex items-center justify-center gap-2 pt-2.5 text-[11px] text-ink3"
                role="status"
              >
                <span className="loading loading-spinner loading-xs" aria-hidden="true" />
                Deleting…
              </div>
            )}
            <button
              type="button"
              className="mt-2 w-full rounded-xl py-2.5 text-[14px] font-semibold text-ink transition-colors duration-150 hover:bg-panel2"
              onClick={onClose}
            >
              Cancel
            </button>
          </m.div>
        </m.div>
      )}
    </AnimatePresence>
  );
}
