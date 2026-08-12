import type { DraftAuditReport, Student } from "../api/types";
import type { MessageKey } from "../i18n/messages";

/**
 * Diagnostics derivation (D6): turn the Rust diagnostics report into the
 * severity-sorted issue list the panel renders. Issues are pure display
 * projections — fixes always dispatch Rust editing commands.
 */

export type DiagnosticIssue = {
  id: string;
  severity: "error" | "warning";
  seatIds: string[];
  titleKey: MessageKey;
  args: Record<string, string | number>;
  /** A concrete fix (student -> seat) when the report could compute one. */
  fix?: { student: string; seatId: string };
};

const WITNESS_TITLES: Record<string, MessageKey> = {
  fixed_seats: "audit.witnessFixedSeat",
  must_be_adjacent: "audit.witnessMustAdjacent",
  cannot_be_adjacent: "audit.witnessCannotAdjacent",
  min_distance: "audit.witnessMinDistance",
};

const SUGGESTION_TITLES: Record<string, MessageKey> = {
  "audit.history_recommended": "audit.historyRecommended",
  "audit.missing_height": "audit.missingHeight",
  "audit.missing_vision": "audit.missingVision",
  "audit.missing_score": "audit.missingScore",
  "audit.ready": "audit.ready",
};

function displayName(students: Student[], id: string): string {
  return students.find((student) => student.id === id)?.name ?? id;
}

/** Rewrite student-key args to display names for the message. */
function localizedArgs(
  args: Record<string, string | number>,
  students: Student[],
): Record<string, string | number> {
  const out: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(args)) {
    if (
      key === "student" ||
      key === "student_a" ||
      key === "student_b"
    ) {
      out[key] = displayName(students, String(value));
    } else {
      out[key] = value;
    }
  }
  return out;
}

export function issuesFromAudit(
  report: DraftAuditReport | null,
  students: Student[],
): DiagnosticIssue[] {
  if (!report) {
    return [];
  }
  const issues: DiagnosticIssue[] = [];

  const witnesses = (report.audit.hard_constraint_summary
    .witnesses ?? []) as Array<Record<string, unknown>>;
  witnesses.forEach((witness, index) => {
    const kind = String(witness.kind ?? "rule");
    const seatIds = Array.isArray(witness.seat_ids)
      ? witness.seat_ids.map(String)
      : [];
    const args = localizedArgs(
      (witness.args as Record<string, string | number>) ?? {},
      students,
    );
    const fixValue = witness.suggested_fix as
      | { student?: string; seat_id?: string }
      | undefined;
    issues.push({
      id: `witness-${index}`,
      severity: "error",
      seatIds,
      titleKey: WITNESS_TITLES[kind] ?? "audit.witnessUnknown",
      args,
      fix:
        fixValue && fixValue.student && fixValue.seat_id
          ? { student: fixValue.student, seatId: fixValue.seat_id }
          : undefined,
    });
  });

  const suggested = report.audit.suggested_actions ?? [];
  suggested.forEach((action, index) => {
    const key = String(action.message_key ?? "");
    const titleKey = SUGGESTION_TITLES[key];
    if (!titleKey) {
      return;
    }
    if (action.suggested_action === "none") {
      return;
    }
    const args = localizedArgs(
      (action.args as Record<string, string | number>) ?? {},
      students,
    );
    issues.push({
      id: `suggestion-${index}`,
      severity: "warning",
      seatIds: [],
      titleKey,
      args,
    });
  });

  return issues;
}

/** One atomic editing command fixing every fixable witness. */
export function fixAllOperations(
  issues: DiagnosticIssue[],
): Array<{ kind: string; payload: Record<string, string | number> }> {
  const operations: Array<{
    kind: string;
    payload: Record<string, string | number>;
  }> = [];
  for (const issue of issues) {
    if (issue.severity !== "error" || !issue.fix) {
      continue;
    }
    operations.push({
      kind: "move_student",
      payload: {
        student_key: issue.fix.student,
        seat_id: issue.fix.seatId,
      },
    });
  }
  return operations;
}
