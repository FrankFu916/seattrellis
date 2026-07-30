export const themes = ["system", "calm", "native", "focus", "paper"] as const;
export type ThemeName = (typeof themes)[number];

const STORAGE_KEY = "seattrellis-theme";

export function isThemeName(value: string | null): value is ThemeName {
  return value !== null && themes.includes(value as ThemeName);
}

export function getInitialTheme(): ThemeName {
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return isThemeName(stored) ? stored : "system";
}

export function applyTheme(theme: ThemeName): void {
  document.documentElement.dataset.theme = theme;
  window.localStorage.setItem(STORAGE_KEY, theme);
}

