// Typing indicator publishing: one start message per burst, cleared 2.5 s
// after the last keystroke, plus on thread switch/unmount so the other
// participant never stays flagged until the server-side expiry.

import { useEffect, useRef } from "react";

export function useTypingIndicator(
  openKey: string,
  virtualKey: boolean,
  value: string,
  sendTyping: (threadId: string, active: boolean) => void,
): void {
  /** True while the backend has been told this thread is typing. */
  const sent = useRef(false);

  useEffect(() => {
    if (openKey.length === 0 || virtualKey) return;
    if (value.trim().length === 0) {
      if (sent.current) {
        sent.current = false;
        sendTyping(openKey, false);
      }
      return;
    }
    if (!sent.current) {
      sent.current = true;
      sendTyping(openKey, true);
    }
    const timer = setTimeout(() => {
      sent.current = false;
      sendTyping(openKey, false);
    }, 2500);
    return () => clearTimeout(timer);
  }, [value, openKey, virtualKey, sendTyping]);

  useEffect(() => {
    return () => {
      if (sent.current) {
        sent.current = false;
        sendTyping(openKey, false);
      }
    };
  }, [openKey, sendTyping]);
}
