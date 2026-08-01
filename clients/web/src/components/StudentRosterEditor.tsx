import { useMemo } from "react";

import type { Student } from "../api/types";
import type { Translate } from "../i18n/messages";

type StudentRosterEditorProps = {
  students: Student[];
  t: Translate;
  onChange: (students: Student[]) => void;
};

export function rosterIsValid(students: Student[]): boolean {
  const seen = new Set<string>();
  return students.every((student) => {
    const id = student.id.trim();
    const name = student.name.trim();
    if (!id || !name || seen.has(id)) {
      return false;
    }
    seen.add(id);
    return true;
  });
}

function newStudentId(students: Student[]): string {
  const used = new Set(students.map((student) => student.id));
  let number = students.length + 1;
  while (used.has(`S${String(number).padStart(2, "0")}`)) {
    number += 1;
  }
  return `S${String(number).padStart(2, "0")}`;
}

function optionalNumber(value: string): number | null {
  const text = value.trim();
  if (!text) {
    return null;
  }
  const parsed = Number(text);
  return Number.isFinite(parsed) ? parsed : null;
}

function splitList(value: string): string[] {
  return value
    .split(/[,，\n]/u)
    .map((item) => item.trim())
    .filter(Boolean);
}

function joinList(value: string[] | undefined): string {
  return (value ?? []).join(", ");
}

export function StudentRosterEditor({
  students,
  t,
  onChange,
}: StudentRosterEditorProps) {
  const invalidRows = useMemo(() => {
    const seen = new Set<string>();
    const invalid = new Set<number>();
    students.forEach((student, index) => {
      const id = student.id.trim();
      const name = student.name.trim();
      if (!id || !name || seen.has(id)) {
        invalid.add(index);
      }
      if (id) {
        seen.add(id);
      }
    });
    return invalid;
  }, [students]);

  function updateStudent(index: number, changes: Partial<Student>): void {
    onChange(
      students.map((student, itemIndex) =>
        itemIndex === index ? { ...student, ...changes } : student,
      ),
    );
  }

  function addStudent(): void {
    onChange([
      ...students,
      {
        id: newStudentId(students),
        name: "",
        tags: [],
        needs: [],
      },
    ]);
  }

  function removeStudent(index: number): void {
    onChange(students.filter((_, itemIndex) => itemIndex !== index));
  }

  return (
    <section className="student-editor-card" aria-labelledby="student-editor-title">
      <div className="student-editor-heading">
        <div>
          <h3 id="student-editor-title">{t("studentEditor.title")}</h3>
          <p>{t("studentEditor.hint")}</p>
        </div>
        <button
          className="secondary-button"
          type="button"
          onClick={addStudent}
          data-testid="student-editor-add"
        >
          {t("studentEditor.add")}
        </button>
      </div>

      <div className="student-editor-table" role="table">
        <div className="student-editor-row student-editor-header" role="row">
          <span role="columnheader">{t("studentEditor.id")}</span>
          <span role="columnheader">{t("studentEditor.name")}</span>
          <span role="columnheader">{t("studentEditor.score")}</span>
          <span role="columnheader">{t("studentEditor.height")}</span>
          <span role="columnheader">{t("studentEditor.vision")}</span>
          <span role="columnheader">{t("studentEditor.needs")}</span>
          <span role="columnheader">{t("studentEditor.notes")}</span>
          <span className="sr-only">{t("studentEditor.remove")}</span>
        </div>
        {students.map((student, index) => (
          <div
            className={`student-editor-row${invalidRows.has(index) ? " is-invalid" : ""}`}
            key={`${student.id}-${index}`}
            role="row"
          >
            <label>
              <span className="sr-only">{t("studentEditor.id")}</span>
              <input
                aria-label={t("studentEditor.idFor", { index: index + 1 })}
                value={student.id}
                onChange={(event) => updateStudent(index, { id: event.target.value })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.name")}</span>
              <input
                aria-label={t("studentEditor.nameFor", { index: index + 1 })}
                value={student.name}
                onChange={(event) => updateStudent(index, { name: event.target.value })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.score")}</span>
              <input
                type="number"
                inputMode="decimal"
                aria-label={t("studentEditor.scoreFor", { index: index + 1 })}
                value={student.score ?? ""}
                onChange={(event) => updateStudent(index, { score: optionalNumber(event.target.value) })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.height")}</span>
              <input
                type="number"
                inputMode="decimal"
                aria-label={t("studentEditor.heightFor", { index: index + 1 })}
                value={student.heightCm ?? ""}
                onChange={(event) => updateStudent(index, { heightCm: optionalNumber(event.target.value) })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.vision")}</span>
              <input
                aria-label={t("studentEditor.visionFor", { index: index + 1 })}
                value={student.vision ?? ""}
                onChange={(event) => updateStudent(index, { vision: event.target.value || null })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.needs")}</span>
              <input
                aria-label={t("studentEditor.needsFor", { index: index + 1 })}
                value={joinList(student.needs)}
                onChange={(event) => updateStudent(index, { needs: splitList(event.target.value) })}
              />
            </label>
            <label>
              <span className="sr-only">{t("studentEditor.notes")}</span>
              <input
                aria-label={t("studentEditor.notesFor", { index: index + 1 })}
                value={student.notes ?? ""}
                onChange={(event) => updateStudent(index, { notes: event.target.value || null })}
              />
            </label>
            <button
              className="icon-button student-editor-remove"
              type="button"
              aria-label={t("studentEditor.removeFor", { name: student.name || student.id })}
              onClick={() => removeStudent(index)}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      {invalidRows.size > 0 ? (
        <p className="student-editor-error" role="alert">
          {t("studentEditor.invalid")}
        </p>
      ) : null}
    </section>
  );
}
