import type { RecentProject } from "../api/types";
import type {
  ClassContext,
  ContentView,
  SessionClass,
} from "../domain/navigation";
import type { Translate, MessageKey } from "../i18n/messages";
import {
  HistoryIcon,
  LayoutIcon,
  PeopleIcon,
  RulesIcon,
  SchoolIcon,
  WorkspaceIcon,
} from "./icons";

type SidebarProps = {
  activeView: ContentView | null;
  context: ClassContext;
  connection: "loading" | "local" | "demo";
  projects: RecentProject[];
  sessionClasses: SessionClass[];
  t: Translate;
  onSelectView: (view: ContentView) => void;
  onSelectClass: (id: string, name: string) => void;
  onSelectTemp: () => void;
};

const CONTENT_ITEMS: Array<{
  view: ContentView;
  label: MessageKey;
  icon: typeof PeopleIcon;
}> = [
  { view: "roster", label: "nav.roster", icon: PeopleIcon },
  { view: "room", label: "nav.room", icon: LayoutIcon },
  { view: "rules", label: "nav.rules", icon: RulesIcon },
  { view: "history", label: "nav.history", icon: HistoryIcon },
];

export function Sidebar({
  activeView,
  context,
  connection,
  projects,
  sessionClasses,
  t,
  onSelectView,
  onSelectClass,
  onSelectTemp,
}: SidebarProps) {
  const isTemp = context.kind === "temp";
  const isClass = (id: string) => context.kind === "class" && context.id === id;

  return (
    <nav className="sidebar" aria-label={t("nav.label")}>
      <section className="sidebar-section" aria-labelledby="sidebar-classes">
        <h2 id="sidebar-classes" className="sidebar-heading">
          {t("nav.classes")}
        </h2>
        {connection === "local" || sessionClasses.length > 0 ? (
          <ul className="sidebar-list">
            {projects.map((project) => (
              <li key={project.path}>
                <button
                  type="button"
                  className="sidebar-item"
                  data-active={isClass(project.path)}
                  onClick={() => onSelectClass(project.path, project.name)}
                >
                  <SchoolIcon />
                  <span className="sidebar-item-copy">
                    <span>{project.name}</span>
                    <small>{project.path}</small>
                  </span>
                </button>
              </li>
            ))}
            {sessionClasses.map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  className="sidebar-item"
                  data-active={isClass(entry.id)}
                  onClick={() => onSelectClass(entry.id, entry.name)}
                >
                  <SchoolIcon />
                  <span className="sidebar-item-copy">
                    <span>
                      {entry.name}{" "}
                      <em className="sidebar-note">{t("nav.classesSessionHint")}</em>
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="sidebar-empty">{t("nav.classesEmpty")}</p>
        )}
      </section>

      <section className="sidebar-section" aria-labelledby="sidebar-content">
        <h2 id="sidebar-content" className="sidebar-heading">
          {t("nav.content")}
        </h2>
        <ul className="sidebar-list">
          {CONTENT_ITEMS.map((item) => (
            <li key={item.view}>
              <button
                type="button"
                className="sidebar-item"
                data-active={activeView === item.view}
                aria-current={activeView === item.view ? "page" : undefined}
                onClick={() => onSelectView(item.view)}
              >
                <item.icon />
                <span>{t(item.label)}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="sidebar-section" aria-labelledby="sidebar-tasks">
        <h2 id="sidebar-tasks" className="sidebar-heading">
          {t("nav.tasks")}
        </h2>
        <ul className="sidebar-list">
          <li>
            <button
              type="button"
              className="sidebar-item"
              data-active={isTemp}
              aria-current={isTemp ? "page" : undefined}
              onClick={onSelectTemp}
            >
              <WorkspaceIcon />
              <span className="sidebar-item-copy">
                <span>{t("nav.tempWorkspace")}</span>
                <small>{t("nav.tempWorkspaceHint")}</small>
              </span>
            </button>
          </li>
        </ul>
      </section>
    </nav>
  );
}
