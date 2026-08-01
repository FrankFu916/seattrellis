import type { Student } from "../api/types";
import {
  diagnoseRuleSetJson,
  type RuleDiagnostic,
  type RuleDiagnosticCode,
} from "../domain/ruleDiagnostics";
import type { MessageKey, Translate } from "../i18n/messages";

type RuleSetDiagnosticsPanelProps = {
  source: string;
  students: Student[];
  seatIds: string[];
  t: Translate;
};

const CODE_MESSAGES: Record<RuleDiagnosticCode, MessageKey> = {
  invalid_json: "generate.rulesDiagnosticInvalidJson",
  root_object: "generate.rulesDiagnosticRootObject",
  unknown_field: "generate.rulesDiagnosticUnknownField",
  object_required: "generate.rulesDiagnosticObjectRequired",
  array_required: "generate.rulesDiagnosticArrayRequired",
  pair_shape: "generate.rulesDiagnosticPairShape",
  fixed_seat_shape: "generate.rulesDiagnosticFixedSeatShape",
  distance_value: "generate.rulesDiagnosticDistance",
  group_shape: "generate.rulesDiagnosticGroupShape",
  group_members: "generate.rulesDiagnosticGroupMembers",
  group_mode: "generate.rulesDiagnosticGroupMode",
  unknown_student: "generate.rulesDiagnosticUnknownStudent",
  unknown_seat: "generate.rulesDiagnosticUnknownSeat",
  value_type: "generate.rulesDiagnosticValueType",
};

const FIX_MESSAGES: Record<RuleDiagnosticCode, MessageKey> = {
  invalid_json: "generate.rulesSuggestionInvalidJson",
  root_object: "generate.rulesSuggestionRootObject",
  unknown_field: "generate.rulesSuggestionUnknownField",
  object_required: "generate.rulesSuggestionObjectRequired",
  array_required: "generate.rulesSuggestionArrayRequired",
  pair_shape: "generate.rulesSuggestionPairShape",
  fixed_seat_shape: "generate.rulesSuggestionFixedSeatShape",
  distance_value: "generate.rulesSuggestionDistance",
  group_shape: "generate.rulesSuggestionGroupShape",
  group_members: "generate.rulesSuggestionGroupMembers",
  group_mode: "generate.rulesSuggestionGroupMode",
  unknown_student: "generate.rulesSuggestionUnknownStudent",
  unknown_seat: "generate.rulesSuggestionUnknownSeat",
  value_type: "generate.rulesSuggestionValueType",
};

function diagnosticText(diagnostic: RuleDiagnostic, t: Translate): string {
  return t(CODE_MESSAGES[diagnostic.code], { path: diagnostic.path });
}

export function RuleSetDiagnosticsPanel({
  source,
  students,
  seatIds,
  t,
}: RuleSetDiagnosticsPanelProps) {
  const text = source.trim();
  if (!text) {
    return (
      <p className="rules-diagnostics rules-diagnostics-empty" data-testid="rules-diagnostics">
        {t("generate.rulesDiagnosticEmpty")}
      </p>
    );
  }
  const diagnostics = diagnoseRuleSetJson(source, students, seatIds);
  return (
    <div
      className={`rules-diagnostics ${diagnostics.length ? "has-errors" : "is-valid"}`}
      data-testid="rules-diagnostics"
      role={diagnostics.length ? "alert" : "status"}
    >
      <strong>{t("generate.rulesDiagnosticTitle")}</strong>
      {diagnostics.length ? (
        <ul>
          {diagnostics.map((diagnostic, index) => (
            <li key={`${diagnostic.path}-${diagnostic.code}-${index}`}>
              <code>{diagnostic.path}</code>
              <span>
                {diagnosticText(diagnostic, t)}
                <small>{t(FIX_MESSAGES[diagnostic.code])}</small>
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <span>{t("generate.rulesDiagnosticValid")}</span>
      )}
    </div>
  );
}
