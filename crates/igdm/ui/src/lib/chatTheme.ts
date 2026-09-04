// IG thread theme (`theme_data`) → CSS variables for the message pane.
// Colors arrive as 8-digit ARGB hex ("FF1F284A") or plain 6-digit hex.

import type { CSSProperties } from "react";
import type { ThreadTheme } from "../state";

/** "FFRRGGBB"/"RRGGBB" (with or without #) → CSS color, or null if unparsable. */
export function argbToCss(color: string | null | undefined): string | null {
  if (!color) return null;
  const hex = color.trim().replace(/^#/, "");
  if (hex.length === 6 && /^[0-9a-fA-F]{6}$/.test(hex)) {
    return `#${hex}`;
  }
  if (hex.length !== 8 || !/^[0-9a-fA-F]{8}$/.test(hex)) return null;
  const a = parseInt(hex.slice(0, 2), 16) / 255;
  if (a === 1) return `#${hex.slice(2)}`;
  const r = parseInt(hex.slice(2, 4), 16);
  const g = parseInt(hex.slice(4, 6), 16);
  const b = parseInt(hex.slice(6, 8), 16);
  return `rgba(${r}, ${g}, ${b}, ${a.toFixed(3)})`;
}

/** True when the app itself is currently in dark mode. */
export function appIsDark(): boolean {
  const attr = document.documentElement.dataset.theme;
  if (attr === "igdm-dark") return true;
  if (attr === "igdm-light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Pick the theme variant matching the app's color mode: threads ship a
 * light "NORMAL" theme plus a "DARK" alternative. */
export function pickThreadTheme(
  theme: ThreadTheme | null | undefined,
  dark: boolean,
): ThreadTheme | null {
  if (!theme) return null;
  const want = dark ? "DARK" : "NORMAL";
  const own = (theme.app_color_mode ?? "NORMAL").toUpperCase();
  if (own === want) return theme;
  const alt = (theme.alternative_themes ?? []).find(
    (a) => (a.app_color_mode ?? "").toUpperCase() === want,
  );
  return alt ?? theme;
}

/** Own-bubble background: the theme's gradient colors when it ships a real
 * gradient (2+ colors), a solid when it ships one color, else the app's
 * default gradient. */
export function outBubbleBackground(theme: ThreadTheme): string | null {
  const colors = (theme.gradient_colors ?? []).flatMap((c) => argbToCss(c) ?? []);
  if (colors.length >= 2) {
    const stops = colors.map((c, i) => `${c} ${(i / (colors.length - 1)) * 100}%`);
    return `linear-gradient(135deg, ${stops.join(", ")})`;
  }
  if (colors.length === 1) return colors[0]!;
  return null;
}

export function colorArrayBackground(
  colors: (string | null | undefined)[] | null | undefined,
  diagonal: boolean,
): string | null {
  const valid = (colors ?? []).flatMap((c) => argbToCss(c) ?? []);
  if (valid.length === 0) return null;
  if (valid.length === 1) return valid[0]!;
  const stops = valid.map((c, i) => `${c} ${(i / (valid.length - 1)) * 100}%`);
  return `linear-gradient(${diagonal ? "135deg" : "180deg"}, ${stops.join(", ")})`;
}

function emphasisColor(theme: ThreadTheme): string | null {
  return (
    argbToCss(theme.emphasized_action_color) ??
    argbToCss(theme.emphasis_colors?.[0]) ??
    argbToCss(theme.fallback_color)
  );
}

function withAlpha(cssColor: string | null, alpha: number): string | null {
  if (!cssColor) return null;
  if (!cssColor.startsWith("#")) return cssColor;
  const r = parseInt(cssColor.slice(1, 3), 16);
  const g = parseInt(cssColor.slice(3, 5), 16);
  const b = parseInt(cssColor.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function contrastOn(cssColor: string): string {
  if (!cssColor.startsWith("#")) return "#ffffff";
  const r = parseInt(cssColor.slice(1, 3), 16);
  const g = parseInt(cssColor.slice(3, 5), 16);
  const b = parseInt(cssColor.slice(5, 7), 16);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 140 ? "#1a1a1a" : "#ffffff";
}

/** Best background-art URL: the smallest asset that covers the window at
 * device pixel ratio, else the largest available. */
export function themeBackgroundUrl(theme: ThreadTheme): string | null {
  const asset = theme.thread_background_asset;
  if (!asset) return null;
  const sizes: { key: keyof typeof asset; size: number }[] = [
    { key: "four_hundred_eighty", size: 480 },
    { key: "seven_hundred_twenty", size: 720 },
    { key: "one_thousand_twenty_four", size: 1024 },
    { key: "two_thousand_forty_eight", size: 2048 },
  ];
  const need = Math.min(2048, (window.innerWidth || 1024) * (window.devicePixelRatio || 1));
  let best: string | null = null;
  for (const s of sizes) {
    const url = asset[s.key];
    if (url) {
      best = url;
      if (s.size >= need) break;
    }
  }
  return best;
}

/** CSS variable overrides for the `.chat-scope` wrapper. Only set when the
 * theme actually provides the value; defaults live in styles.css. */
export function threadThemeVars(theme: ThreadTheme): CSSProperties {
  const out = outBubbleBackground(theme);
  const sendBg =
    colorArrayBackground(theme.composer_send_button_colors, false) ??
    out ??
    argbToCss(theme.fallback_color);
  const circleBg =
    colorArrayBackground(
      theme.composer_circle_button_colors,
      theme.should_use_diagonal_gradient_for_composer_circle_button === true,
    ) ?? argbToCss(theme.fallback_color);
  const glass = argbToCss(theme.blurred_composer_background_color);
  const hasArt = themeBackgroundUrl(theme) !== null;
  const vars: Record<string, string> = {};
  const set = (name: string, value: string | null | undefined) => {
    if (value) vars[name] = value;
  };
  set("--ct-bg", argbToCss(theme.thread_background_color));
  set("--ct-bubble-in", argbToCss(theme.incoming_message_bubble_color));
  set("--ct-text-in", argbToCss(theme.inbound_message_text_color));
  set("--ct-text-out", argbToCss(theme.outbound_message_text_color));
  if (out) vars["--ct-bubble-out"] = out;
  const quoteBg = argbToCss(theme.quoted_incoming_message_bubble_color);
  set("--ct-quote-in", quoteBg);
  // Quote-bubble text reads on `--ct-quote-in`, so it must contrast with it:
  // prefer the theme's dedicated quote text color, else derive black/white
  // from the quote background luminance (a light quote never gets light text).
  const quoteText =
    argbToCss(theme.quoted_incoming_message_text_color) ??
    (quoteBg && quoteBg.startsWith("#") ? contrastOn(quoteBg) : undefined);
  set("--ct-quote-text", quoteText);
  set("--ct-reaction", argbToCss(theme.reaction_pill_color));
  set("--ct-secondary", argbToCss(theme.secondary_text_color));
  set("--ct-separator", argbToCss(theme.solid_separator_color));
  set("--ct-header-bg", argbToCss(theme.navigation_bar_color));
  set("--ct-header-title", argbToCss(theme.navigation_bar_title_color));
  set("--ct-header-subtitle", argbToCss(theme.navigation_bar_subtitle_color));
  set("--ct-input-bg", argbToCss(theme.composer_input_background_color));
  set("--ct-input-text", argbToCss(theme.inbound_message_text_color));
  set(
    "--ct-placeholder",
    argbToCss(theme.composer_placeholder_text_color) ??
      withAlpha(argbToCss(theme.inbound_message_text_color), 0.45),
  );
  set("--ct-icon", argbToCss(theme.composer_secondary_button_color));
  set("--ct-composer-bg", argbToCss(theme.solid_composer_background_color));
  if (sendBg) vars["--ct-send-bg"] = sendBg;
  set("--ct-send-fg", argbToCss(theme.outbound_message_text_color));
  if (circleBg) vars["--ct-circle-btn"] = circleBg;
  const circleRef =
    (theme.composer_circle_button_colors ?? []).map(argbToCss).find(Boolean) ??
    argbToCss(theme.fallback_color) ??
    argbToCss(theme.composer_input_background_color);
  set(
    "--ct-circle-icon",
    // The circle-button icon must read on the button's own background, so
    // derive it from contrast against the circle color. The theme's
    // secondary-button color is for other controls and can be same-hue as a
    // dark circle (dark icon on dark button); only fall back to it when the
    // circle color isn't a solid hex we can compute contrast on.
    circleRef && circleRef.startsWith("#")
      ? contrastOn(circleRef)
      : argbToCss(theme.composer_secondary_button_color),
  );
  set("--ct-emphasis", emphasisColor(theme));
  if (hasArt && glass) {
    vars["--ct-glass-bg"] = glass;
    vars["--ct-blur-px"] = "20px";
  }
  // SAFETY: vars maps CSS custom properties (`--ct-*`) to string colors,
  // which React accepts as CSSProperties.
  return vars as CSSProperties;
}
