import { useMemo, useState } from "react";

import type { CommonConstraint, Student } from "../api/types";
import type { Translate } from "../i18n/messages";

type BulkConstraintKind = CommonConstraint["kind"];

type BulkConstraintEditorProps = {
  students: Student[];
  seatIds: string[];
  existingConstraints: CommonConstraint[];
  t: Translate;
  onAdd: (constraints: CommonConstraint[]) => void;
};

type ParsedConstraint = {
  constraint: CommonConstraint;
  line: number;
};

type ParseIssue = {
  line: number;
  reason: "columns" | "student" | "seat" | "same" | "duplicate";
  value?: string;
};

function newConstraintId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function splitLine(line: string): string[] {
  return line
    .replace(/\s*[-=]>\s*/gu, ",")
    .split(/[,;|\t]+/u)
    .map((value) => value.trim())
    .filter(Boolean);
}

function constraintKey(constraint: CommonConstraint): string {
  if (constraint.kind === "fixed_seat") {
    return `${constraint.kind}|${constraint.first}|${constraint.seatId}`;
  }
  const pair = [constraint.first, constraint.second].sort();
  return `${constraint.kind}|${pair.join("|")}`;
}

function parseConstraints(
  source: string,
  kind: BulkConstraintKind,
  distance: number,
  metric: CommonConstraint["metric"],
  students: Student[],
  seatIds: string[],
  existingConstraints: CommonConstraint[],
): { valid: ParsedConstraint[]; issues: ParseIssue[] } {
  const studentIds = new Set(students.map((student) => student.id));
  const availableSeats = new Set(seatIds);
  const knownKeys = new Set(existingConstraints.map(constraintKey));
  const valid: ParsedConstraint[] = [];
  const issues: ParseIssue[] = [];
  const seenKeys = new Set<string>();

  source.split(/\r?\n/u).forEach((rawLine, index) => {
    const lineNumber = index + 1;
    const line = rawLine.replace(/#.*/u, "").trim();
    if (!line) return;
    const values = splitLine(line);
    if (values.length !== 2) {
      issues.push({ line: lineNumber, reason: "columns" });
      return;
    }
    const [first, second] = values;
    if (!studentIds.has(first)) {
      issues.push({ line: lineNumber, reason: "student", value: first });
      return;
    }
    if (kind === "fixed_seat") {
      if (!availableSeats.has(second)) {
        issues.push({ line: lineNumber, reason: "seat", value: second });
        return;
      }
    } else {
      if (!studentIds.has(second)) {
        issues.push({ line: lineNumber, reason: "student", value: second });
        return;
      }
      if (first === second) {
        issues.push({ line: lineNumber, reason: "same", value: first });
        return;
      }
    }

    const constraint: CommonConstraint = {
      id: newConstraintId(),
      kind,
      first,
      second: kind === "fixed_seat" ? "" : second,
      seatId: kind === "fixed_seat" ? second : "",
      distance,
      metric,
    };
    const key = constraintKey(constraint);
    if (knownKeys.has(key) || seenKeys.has(key)) {
      issues.push({ line: lineNumber, reason: "duplicate" });
      return;
    }
    seenKeys.add(key);
    valid.push({ constraint, line: lineNumber });
  });

  return { valid, issues };
}

function issueLabel(issue: ParseIssue, t: Translate): string {
  switch (issue.reason) {
    case "columns":
      return t("bulkRules.issueColumns", { line: issue.line });
    case "student":
      return t("bulkRules.issueStudent", { line: issue.line, value: issue.value ?? "" });
    case "seat":
      return t("bulkRules.issueSeat", { line: issue.line, value: issue.value ?? "" });
    case "same":
      return t("bulkRules.issueSame", { line: issue.line, value: issue.value ?? "" });
    default:
      return t("bulkRules.issueDuplicate", { line: issue.line });
  }
}

export function BulkConstraintEditor({
  students,
  seatIds,
  existingConstraints,
  t,
  onAdd,
}: BulkConstraintEditorProps) {
  const [kind, setKind] = useState<BulkConstraintKind>("avoid_adjacent");
  const [source, setSource] = useState("");
  const [distance, setDistance] = useState(2);
  const [metric, setMetric] = useState<CommonConstraint["metric"]>("graph");
  const parsed = useMemo(
    () =>
      parseConstraints(
        source,
        kind,
        distance,
        metric,
        students,
        seatIds,
        existingConstraints,
      ),
    [source, kind, distance, metric, students, seatIds, existingConstraints],
  );

  function handleAdd(): void {
    if (!parsed.valid.length) return;
    onAdd(parsed.valid.map((item) => item.constraint));
    setSource("");
  }

  return (
    <details className="bulk-constraint-editor" data-testid="bulk-constraint-editor">
      <summary>{t("bulkRules.title")}</summary>
      <p className="constraint-help">{t("bulkRules.hint")}</p>
      <div className="bulk-constraint-fields">
        <label className="advanced-field">
          <span>{t("bulkRules.type")}</span>
          <select
            data-testid="bulk-constraint-kind"
            value={kind}
            onChange={(event) => setKind(event.target.value as BulkConstraintKind)}
          >
            <option value="avoid_adjacent">{t("constraints.avoidAdjacent")}</option>
            <option value="must_adjacent">{t("constraints.mustAdjacent")}</option>
            <option value="min_distance">{t("constraints.minDistance")}</option>
            <option value="fixed_seat">{t("constraints.fixedSeat")}</option>
          </select>
        </label>
        {kind === "min_distance" && (
          <div className="bulk-constraint-options">
            <label className="advanced-field">
              <span>{t("constraints.distance")}</span>
              <input
                type="number"
                min={0.1}
                step={0.1}
                value={distance}
                onChange={(event) => setDistance(Math.max(0.1, Number(event.target.value) || 0.1))}
              />
            </label>
            <label className="advanced-field">
              <span>{t("constraints.metric")}</span>
              <select
                value={metric}
                onChange={(event) => setMetric(event.target.value as CommonConstraint["metric"])}
              >
                <option value="graph">{t("constraints.metricGraph")}</option>
                <option value="euclidean">{t("constraints.metricEuclidean")}</option>
              </select>
            </label>
          </div>
        )}
        <label className="advanced-field advanced-field-wide">
          <span>{t("bulkRules.input")}</span>
          <textarea
            data-testid="bulk-constraint-input"
            rows={4}
            value={source}
            placeholder={t("bulkRules.placeholder")}
            onChange={(event) => setSource(event.target.value)}
          />
          <small>{t("bulkRules.inputHint")}</small>
        </label>
      </div>
      {source.trim() && (
        <div className="bulk-constraint-preview" data-testid="bulk-constraint-preview">
          <strong>
            {t("bulkRules.preview", { count: parsed.valid.length })}
          </strong>
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
              {parsed.valid.slice(0, 6).map(({ constraint, line }) => (
                <li key={constraint.id}>
                  {t("bulkRules.previewLine", {
                    line,
                    first: constraint.first,
                    second: constraint.kind === "fixed_seat" ? constraint.seatId : constraint.second,
                  })}
                </li>
              ))}
              {parsed.valid.length > 6 && (
                <li>{t("bulkRules.previewMore", { count: parsed.valid.length - 6 })}</li>
              )}
            </ul>
          )}
          <button
            className="secondary-button"
            type="button"
            data-testid="bulk-constraint-apply"
            disabled={!parsed.valid.length}
            onClick={handleAdd}
          >
            {t("bulkRules.apply", { count: parsed.valid.length })}
          </button>
        </div>
      )}
    </details>
  );
}

