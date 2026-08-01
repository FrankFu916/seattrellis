import { useMemo, useState } from "react";

import type { CommonGroupRule, Student } from "../api/types";
import type { Translate } from "../i18n/messages";

type BulkGroupEditorProps = {
  students: Student[];
  existingGroups: CommonGroupRule[];
  t: Translate;
  onAdd: (groups: CommonGroupRule[]) => void;
};

type GroupIssueReason = "format" | "name" | "students" | "unknown" | "duplicate";

type GroupIssue = {
  line: number;
  reason: GroupIssueReason;
  value?: string;
};

type ParsedGroup = {
  group: CommonGroupRule;
  line: number;
};

function newGroupId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function parseGroups(
  source: string,
  mode: CommonGroupRule["mode"],
  students: Student[],
  existingGroups: CommonGroupRule[],
): { valid: ParsedGroup[]; issues: GroupIssue[] } {
  const studentIds = new Set(students.map((student) => student.id));
  const names = new Set(existingGroups.map((group) => group.name.trim().toLocaleLowerCase()));
  const valid: ParsedGroup[] = [];
  const issues: GroupIssue[] = [];

  source.split(/\r?\n/u).forEach((rawLine, index) => {
    const lineNumber = index + 1;
    const line = rawLine.replace(/#.*/u, "").trim();
    if (!line) return;
    const separator = line.indexOf(":");
    if (separator < 1) {
      issues.push({ line: lineNumber, reason: "format" });
      return;
    }
    const name = line.slice(0, separator).trim();
    const members = Array.from(
      new Set(
        line
          .slice(separator + 1)
          .split(/[,，;|\t]+/u)
          .map((value) => value.trim())
          .filter(Boolean),
      ),
    );
    if (!name) {
      issues.push({ line: lineNumber, reason: "name" });
      return;
    }
    if (names.has(name.toLocaleLowerCase())) {
      issues.push({ line: lineNumber, reason: "duplicate", value: name });
      return;
    }
    if (members.length < 2) {
      issues.push({ line: lineNumber, reason: "students" });
      return;
    }
    const unknown = members.find((student) => !studentIds.has(student));
    if (unknown) {
      issues.push({ line: lineNumber, reason: "unknown", value: unknown });
      return;
    }
    names.add(name.toLocaleLowerCase());
    valid.push({
      line: lineNumber,
      group: { id: newGroupId(), name, mode, students: members },
    });
  });

  return { valid, issues };
}

function issueLabel(issue: GroupIssue, t: Translate): string {
  switch (issue.reason) {
    case "format":
      return t("bulkGroups.issueFormat", { line: issue.line });
    case "name":
      return t("bulkGroups.issueName", { line: issue.line });
    case "students":
      return t("bulkGroups.issueStudents", { line: issue.line });
    case "unknown":
      return t("bulkGroups.issueUnknown", { line: issue.line, value: issue.value ?? "" });
    default:
      return t("bulkGroups.issueDuplicate", { line: issue.line, value: issue.value ?? "" });
  }
}

export function BulkGroupEditor({
  students,
  existingGroups,
  t,
  onAdd,
}: BulkGroupEditorProps) {
  const [mode, setMode] = useState<CommonGroupRule["mode"]>("separate");
  const [source, setSource] = useState("");
  const parsed = useMemo(
    () => parseGroups(source, mode, students, existingGroups),
    [source, mode, students, existingGroups],
  );

  function handleAdd(): void {
    if (!parsed.valid.length) return;
    onAdd(parsed.valid.map((item) => item.group));
    setSource("");
  }

  return (
    <details className="bulk-group-editor" data-testid="bulk-group-editor">
      <summary>{t("bulkGroups.title")}</summary>
      <p className="constraint-help">{t("bulkGroups.hint")}</p>
      <label className="advanced-field">
        <span>{t("groups.mode")}</span>
        <select
          data-testid="bulk-group-mode"
          value={mode}
          onChange={(event) => setMode(event.target.value as CommonGroupRule["mode"])}
        >
          <option value="separate">{t("groups.separate")}</option>
          <option value="together">{t("groups.together")}</option>
        </select>
      </label>
      <label className="advanced-field advanced-field-wide">
        <span>{t("bulkGroups.input")}</span>
        <textarea
          data-testid="bulk-group-input"
          rows={4}
          value={source}
          placeholder={t("bulkGroups.placeholder")}
          onChange={(event) => setSource(event.target.value)}
        />
        <small>{t("bulkGroups.inputHint")}</small>
      </label>
      {source.trim() && (
        <div className="bulk-group-preview" data-testid="bulk-group-preview">
          <strong>{t("bulkGroups.preview", { count: parsed.valid.length })}</strong>
          {parsed.issues.length > 0 && (
            <ul className="bulk-constraint-issues">
              {parsed.issues.map((issue) => (
                <li key={`${issue.line}-${issue.reason}-${issue.value ?? ""}`}>
                  {issueLabel(issue, t)}
                </li>
              ))}
            </ul>
          )}
          {parsed.valid.length > 0 && (
            <ul className="bulk-constraint-valid">
              {parsed.valid.slice(0, 6).map(({ group, line }) => (
                <li key={group.id}>
                  {t("bulkGroups.previewLine", {
                    line,
                    name: group.name,
                    count: group.students.length,
                  })}
                </li>
              ))}
              {parsed.valid.length > 6 && (
                <li>{t("bulkGroups.previewMore", { count: parsed.valid.length - 6 })}</li>
              )}
            </ul>
          )}
          <button
            className="secondary-button"
            type="button"
            data-testid="bulk-group-apply"
            disabled={!parsed.valid.length}
            onClick={handleAdd}
          >
            {t("bulkGroups.apply", { count: parsed.valid.length })}
          </button>
        </div>
      )}
    </details>
  );
}

