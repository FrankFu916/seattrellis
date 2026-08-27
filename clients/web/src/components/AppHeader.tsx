import type { Locale, Translate } from "../i18n/messages";

type AppHeaderProps = {
  locale: Locale;
  connection: "loading" | "local" | "demo";
  t: Translate;
  onLocaleChange: (locale: Locale) => void;
};

export function AppHeader({
  locale,
  connection,
  t,
  onLocaleChange,
}: AppHeaderProps) {
  const connectionLabel =
    connection === "loading"
      ? t("app.loading")
      : connection === "local"
        ? t("app.localReady")
        : t("app.demoReady");

  return (
    <header className="app-header">
      <div className="brand-lockup" aria-label={`${t("app.name")} ${t("app.product")}`}>
        <svg
          className="brand-mark"
          viewBox="0 0 36 36"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="brand-c1" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#2dd4bf" />
              <stop offset="1" stopColor="#0d9488" />
            </linearGradient>
            <linearGradient id="brand-c2" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#60a5fa" />
              <stop offset="1" stopColor="#2563eb" />
            </linearGradient>
            <linearGradient id="brand-c3" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#a78bfa" />
              <stop offset="1" stopColor="#7c3aed" />
            </linearGradient>
            <linearGradient id="brand-c4" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#f472b6" />
              <stop offset="1" stopColor="#db2777" />
            </linearGradient>
            <linearGradient id="brand-c5" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#fbbf24" />
              <stop offset="1" stopColor="#f59e0b" />
            </linearGradient>
          </defs>
          <circle
            cx="18"
            cy="18"
            r="13.4"
            fill="none"
            stroke="currentColor"
            strokeOpacity="0.28"
            strokeWidth="1.1"
          />
          <g>
            <rect x="14.9" y="1.4" width="6.2" height="4.9" rx="1.6" fill="url(#brand-c1)" />
            <g transform="rotate(60 18 18)">
              <rect x="14.9" y="1.4" width="6.2" height="4.9" rx="1.6" fill="url(#brand-c2)" />
            </g>
            <g transform="rotate(120 18 18)">
              <rect x="14.9" y="1.4" width="6.2" height="4.9" rx="1.6" fill="url(#brand-c3)" />
            </g>
            <g transform="rotate(180 18 18)">
              <rect
                x="14.9"
                y="1.4"
                width="6.2"
                height="4.9"
                rx="1.6"
                fill="none"
                stroke="currentColor"
                strokeOpacity="0.55"
                strokeWidth="0.8"
                strokeDasharray="1.6 1.2"
              />
            </g>
            <g transform="rotate(240 18 18)">
              <rect x="14.9" y="1.4" width="6.2" height="4.9" rx="1.6" fill="url(#brand-c4)" />
            </g>
            <g transform="rotate(300 18 18)">
              <rect x="14.9" y="1.4" width="6.2" height="4.9" rx="1.6" fill="url(#brand-c5)" />
            </g>
          </g>
          <circle cx="18" cy="18" r="2.5" fill="currentColor" />
        </svg>
        <span>
          <strong>{t("app.name")}</strong>
          <small>{t("app.product")}</small>
        </span>
      </div>

      <div className="header-preferences">
        <span className={`connection-pill connection-${connection}`}>
          <i aria-hidden="true" />
          {connectionLabel}
        </span>
        <label>
          <span>{t("header.language")}</span>
          <select
            value={locale}
            onChange={(event) =>
              onLocaleChange(event.target.value as Locale)
            }
          >
            <option value="zh-CN">{t("header.zh")}</option>
            <option value="en">{t("header.en")}</option>
          </select>
        </label>
      </div>
    </header>
  );
}
