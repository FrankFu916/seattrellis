import type { Student } from "../api/types";
import type { Translate } from "../i18n/messages";

type UnseatedTrayProps = {
  students: Student[];
  t: Translate;
};

export function UnseatedTray({ students, t }: UnseatedTrayProps) {
  return (
    <section className="side-card unseated-card" aria-labelledby="unseated-title">
      <header>
        <span className="card-icon" aria-hidden="true">
          +
        </span>
        <div>
          <h2 id="unseated-title">{t("unseated.title")}</h2>
          <p>
            {students.length === 0
              ? t("unseated.empty")
              : t("unseated.count", { count: students.length })}
          </p>
        </div>
      </header>
      {students.length > 0 ? (
        <ul className="student-chips" aria-live="polite">
          {students.map((student) => (
            <li key={student.id}>{student.name}</li>
          ))}
        </ul>
      ) : (
        <div className="empty-state-mark" aria-hidden="true">
          ✓
        </div>
      )}
    </section>
  );
}

