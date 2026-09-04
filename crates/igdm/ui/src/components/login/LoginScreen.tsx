import { useState } from "react";
import { useApp } from "../../hooks/useApp";

export default function LoginScreen() {
  const { state, loginPassword, loginSessionid, loginSaved, provideCode, cancelCode } = useApp();
  const login = state.login;

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [sessionid, setSessionid] = useState("");
  const [code, setCode] = useState("");

  const doLogin = () => {
    const u = username.trim();
    if (u.length === 0 || password.length === 0) {
      setErrorMsg("Enter your username and password.");
      return;
    }
    loginPassword(u, password);
  };

  const doSessionid = () => {
    const sid = sessionid.trim();
    if (sid.length < 30) {
      setErrorMsg("That doesn't look like a valid sessionid (too short).");
      return;
    }
    loginSessionid(sid);
  };

  const doSubmitCode = () => {
    const c = code.trim();
    if (c.length === 0) return;
    provideCode(c);
  };

  const doCancelCode = () => {
    cancelCode();
  };

  const [errorMsg, setErrorMsg] = useState("");

  const error = errorMsg || login.error;

  if (login.show_code) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-bg">
        <div className="flex w-[420px] flex-col gap-2.5 rounded-2xl border border-border bg-panel p-6">
          <div className="flex justify-center">
            <Logo size={56} />
          </div>
          <h1 className="w-full text-center text-[22px] font-bold text-ink">
            Verification required
          </h1>
          <p className="w-full text-center text-[13px] text-ink2">{login.code_info}</p>
          <div className="h-2" />
          <input
            className="w-full rounded-lg border border-border bg-panel2 px-3 py-2 text-ink outline-none focus:border-accent"
            placeholder="6-digit code"
            aria-label="Verification code"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") doSubmitCode();
            }}
            autoFocus
          />
          <button className="btn btn-primary w-full" disabled={login.busy} onClick={doSubmitCode}>
            Confirm
          </button>
          <button className="btn btn-ghost w-full" onClick={doCancelCode}>
            Cancel
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full w-full items-center justify-center bg-bg">
      <div className="flex w-[420px] flex-col gap-2.5 rounded-2xl border border-border bg-panel p-6">
        <div className="flex justify-center">
          <Logo size={56} />
        </div>
        <h1 className="w-full text-center text-[22px] font-bold text-ink">Instagram Direct</h1>
        <p className="w-full text-center text-[13px] text-ink2">Sign in to your messages</p>
        <div className="h-2" />
        <input
          className="w-full rounded-lg border border-border bg-panel2 px-3 py-2 text-ink outline-none focus:border-accent"
          placeholder="Username, phone or email"
          aria-label="Username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <input
          className="w-full rounded-lg border border-border bg-panel2 px-3 py-2 text-ink outline-none focus:border-accent"
          placeholder="Password"
          aria-label="Password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") doLogin();
          }}
        />
        <button className="btn btn-primary w-full" disabled={login.busy} onClick={doLogin}>
          {login.busy ? "Signing in…" : "Log in"}
        </button>
        <div className="flex w-full items-center gap-2">
          <div className="h-px flex-1 bg-border-soft" />
          <span className="text-[12px] text-ink3">or</span>
          <div className="h-px flex-1 bg-border-soft" />
        </div>
        <span className="text-[11px] font-semibold text-ink3">Session ID login</span>
        <input
          className="w-full rounded-lg border border-border bg-panel2 px-3 py-2 text-ink outline-none focus:border-accent"
          placeholder="sessionid cookie value"
          aria-label="Session ID"
          value={sessionid}
          onChange={(e) => setSessionid(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") doSessionid();
          }}
        />
        <button
          className="btn btn-ghost w-full border border-border text-ink"
          disabled={login.busy}
          onClick={doSessionid}
        >
          {login.busy ? "Signing in…" : "Log in with sessionid"}
        </button>
        <p className="text-[11px] text-ink3">
          Tip: copy the sessionid cookie from instagram.com in your browser (F12 → Application →
          Cookies).
        </p>

        {state.savedSessions.length > 0 && (
          <div className="mt-2 flex flex-col gap-1.5 border-t border-border pt-3">
            <span className="text-[11px] font-semibold text-ink3">Saved sessions</span>
            {state.savedSessions.slice(0, 5).map((name) => {
              const isPending = login.busy && login.pendingSession === name;
              return (
                <button
                  key={name}
                  className="btn btn-ghost w-full border border-border text-left text-[13px] text-ink transition-transform active:scale-[0.98]"
                  disabled={login.busy}
                  onClick={() => loginSaved(name)}
                >
                  {isPending ? (
                    <span className="flex items-center gap-2">
                      <span
                        className="loading loading-spinner loading-xs motion-reduce:animate-none"
                        aria-hidden="true"
                      />
                      Signing in…
                    </span>
                  ) : (
                    name
                  )}
                </button>
              );
            })}
          </div>
        )}

        {error && <p className="w-full text-center text-[12px] text-danger">{error}</p>}
      </div>
    </div>
  );
}

export function Logo({ size }: { size: number }) {
  return (
    <div
      className="brand-gradient flex items-center justify-center rounded-[20%]"
      style={{ width: size, height: size }}
    >
      <svg
        width={size * 0.6}
        height={size * 0.6}
        viewBox="0 0 24 24"
        fill="none"
        stroke="white"
        strokeWidth={1.8}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="2.5" y="2.5" width="19" height="19" rx="5.5" />
        <circle cx="12" cy="12" r="4" />
        <circle cx="17.5" cy="6.5" r="1.1" fill="white" stroke="none" />
      </svg>
    </div>
  );
}
