// Theme application: "system" removes the override (CSS media query takes
// over), "light"/"dark" force a palette. The attribute doubles as the
// daisyUI theme selector, so values are the daisy theme names.

export type Theme = "system" | "light" | "dark";

const DAISY_THEME: Record<Exclude<Theme, "system">, string> = {
  light: "igdm-light",
  dark: "igdm-dark",
};

export function isTheme(v: unknown): v is Theme {
  return v === "system" || v === "light" || v === "dark";
}

export function applyTheme(theme: Theme): void {
  const el = document.documentElement;
  if (theme === "system") {
    el.removeAttribute("data-theme");
  } else {
    el.setAttribute("data-theme", DAISY_THEME[theme]);
  }
}
