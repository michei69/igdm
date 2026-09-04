import { AnimatePresence, LazyMotion, domAnimation, m, useReducedMotion } from "motion/react";
import { Route, Routes } from "react-router";
import { Toaster } from "sonner";
import { useApp } from "./hooks/useApp";
import LoginScreen, { Logo } from "./components/login/LoginScreen";
import MainScreen from "./components/MainScreen";
import SettingsScreen from "./components/settings/SettingsScreen";

const EXPO_OUT = [0.16, 1, 0.3, 1] as const;

function AnimatedScreens() {
  const { state } = useApp();
  const reduce = useReducedMotion();
  const screen = state.screen;

  return (
    <div className="relative h-full w-full overflow-hidden">
      <LazyMotion features={domAnimation}>
        <AnimatePresence initial={false}>
          <m.div
            key={screen}
            className="absolute inset-0"
            initial={reduce ? { opacity: 0 } : { opacity: 0, y: 16 }}
            animate={reduce ? { opacity: 1 } : { opacity: 1, y: 0 }}
            exit={reduce ? { opacity: 0 } : { opacity: 0, y: -8 }}
            transition={reduce ? { duration: 0 } : { duration: 0.28, ease: EXPO_OUT }}
          >
            {screen === "main" ? (
              <MainScreen />
            ) : screen === "booting" ? (
              <BootScreen />
            ) : (
              <LoginScreen />
            )}
          </m.div>
        </AnimatePresence>
      </LazyMotion>
    </div>
  );
}

/** Neutral splash while the last-used session resumes; never shows the login
 * form unless the resume fails (reducer flips to `login`). */
function BootScreen() {
  return (
    <div className="flex h-full w-full items-center justify-center bg-bg">
      <div className="flex flex-col items-center gap-3">
        <Logo size={56} />
        <span className="loading loading-spinner loading-md motion-reduce:animate-none" aria-hidden="true" />
      </div>
    </div>
  );
}

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/settings" element={<SettingsScreen />} />
        <Route path="*" element={<AnimatedScreens />} />
      </Routes>
      <Toaster
        position="bottom-center"
        duration={1000}
        toastOptions={{
          style: {
            background: "var(--ig-panel2)",
            color: "var(--ig-ink)",
            border: "1px solid var(--ig-border)",
            borderRadius: "10px",
            fontSize: "12px",
          },
        }}
      />
    </>
  );
}
