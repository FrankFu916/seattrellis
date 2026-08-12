import type { Locale, Translate } from "../i18n/messages";

type AppHeaderProps = {
  locale: Locale;
  studentCount: number;
  connection: "loading" | "local" | "demo";
  t: Translate;
  onLocaleChange: (locale: Locale) => void;
};

export function AppHeader({
  locale,
  studentCount,
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
          <rect x="3" y="4" width="13" height="12" rx="3" />
          <rect x="20" y="4" width="13" height="12" rx="3" />
          <rect x="3" y="20" width="13" height="12" rx="3" />
          <path d="M21 26h11M26.5 20.5v11" />
        </svg>
        <span>
          <strong>{t("app.name")}</strong>
          <small>{t("app.product")}</small>
        </span>
      </div>

      <div className="class-summary">
        <strong>{t("app.className")}</strong>
        <span>{t("app.students", { count: studentCount })}</span>
        <span className={`connection-pill connection-${connection}`}>
          <i aria-hidden="true" />
          {connectionLabel}
        </span>
      </div>

      <div className="header-preferences">
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
