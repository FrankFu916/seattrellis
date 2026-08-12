import type {
  CommonConstraint,
  CommonGroupRule,
  CommonPreferenceId,
  Student,
} from "../api/types";
import type { Locale, MessageKey, Translate } from "../i18n/messages";

type RuleCardsProps = {
  constraints: CommonConstraint[];
  groups: CommonGroupRule[];
  preferences: CommonPreferenceId[];
  students: Student[];
  locale: Locale;
  t: Translate;
  onConstraintToggle: (id: string, enabled: boolean) => void;
  onConstraintRemove: (id: string) => void;
  onConstraintEdit: (id: string) => void;
  onGroupToggle: (id: string, enabled: boolean) => void;
  onGroupRemove: (id: string) => void;
  onGroupEdit: (id: string) => void;
  onPreferenceToggle: (id: CommonPreferenceId) => void;
};

const CONSTRAINT_LABELS: Record<CommonConstraint["kind"], MessageKey> = {
  avoid_adjacent: "constraints.avoidAdjacent",
  must_adjacent: "constraints.mustAdjacent",
  fixed_seat: "constraints.fixedSeat",
  min_distance: "constraints.minDistance",
};

const PREFERENCE_META: Array<{
  id: CommonPreferenceId;
  label: MessageKey;
  description: MessageKey;
}> = [
  { id: "vision_front", label: "preference.visionFront", description: "preference.visionFrontHint" },
  { id: "height_back", label: "preference.heightBack", description: "preference.heightBackHint" },
  { id: "fair_rotation", label: "preference.fairRotation", description: "preference.fairRotationHint" },
  { id: "avoid_recent_neighbors", label: "preference.avoidNeighbors", description: "preference.avoidNeighborsHint" },
  { id: "score_position", label: "preference.scorePosition", description: "preference.scorePositionHint" },
  { id: "score_distribution", label: "preference.scoreDistribution", description: "preference.scoreDistributionHint" },
  { id: "mentor_pairing", label: "preference.mentorPairing", description: "preference.mentorPairingHint" },
];

function studentName(students: Student[], id: string): string {
  return students.find((student) => student.id === id)?.name ?? id;
}

function constraintDescription(
  constraint: CommonConstraint,
  students: Student[],
  locale: Locale,
): string {
  const a = studentName(students, constraint.first);
  const b = studentName(students, constraint.second);
  switch (constraint.kind) {
    case "fixed_seat":
      return `${a} → ${constraint.seatId}`;
    case "min_distance":
      return `${a} · ${b} ≥ ${constraint.distance}`;
    default:
      return `${a} · ${b}`;
  }
}

/** Rule card list (D3): manage (enable/disable, edit, delete) existing rules. */
export function RuleCards({
  constraints,
  groups,
  preferences,
  students,
  locale,
  t,
  onConstraintToggle,
  onConstraintRemove,
  onConstraintEdit,
  onGroupToggle,
  onGroupRemove,
  onGroupEdit,
  onPreferenceToggle,
}: RuleCardsProps) {
  const hardCount = constraints.length + groups.length;
  const softCount = preferences.length;

  return (
    <div className="rule-cards">
      <div className="rule-section-heading">
        <span className="chip chip-red">{t("rules.hard")}</span>
        <span className="muted small">
          {t("rules.hardHint")}
          <span className="rule-count">{hardCount}</span>
        </span>
      </div>
      {hardCount === 0 ? <p className="muted">{t("rules.hardEmpty")}</p> : null}
      {constraints.map((constraint) => (
        <div
          className="rule-card rule-card-hard"
          data-disabled={constraint.enabled === false}
          key={constraint.id}
        >
          <div className="rc-head">
            <label className="rc-toggle">
              <input
                type="checkbox"
                checked={constraint.enabled !== false}
                onChange={(event) =>
                  onConstraintToggle(constraint.id, event.target.checked)
                }
              />
              <span className="rc-title">
                {t(CONSTRAINT_LABELS[constraint.kind])}
              </span>
            </label>
            <span className="rc-actions">
              <button
                type="button"
                className="text-button"
                onClick={() => onConstraintEdit(constraint.id)}
              >
                {t("rules.edit")}
              </button>
              <button
                type="button"
                className="text-button danger"
                onClick={() => onConstraintRemove(constraint.id)}
              >
                {t("rules.delete")}
              </button>
            </span>
          </div>
          <div className="rc-desc">
            {constraintDescription(constraint, students, locale)}
          </div>
        </div>
      ))}
      {groups.map((group) => (
        <div
          className="rule-card rule-card-hard"
          data-disabled={group.enabled === false}
          key={group.id}
        >
          <div className="rc-head">
            <label className="rc-toggle">
              <input
                type="checkbox"
                checked={group.enabled !== false}
                onChange={(event) =>
                  onGroupToggle(group.id, event.target.checked)
                }
              />
              <span className="rc-title">{t("groups.title")} · {group.name}</span>
            </label>
            <span className="rc-actions">
              <button
                type="button"
                className="text-button"
                onClick={() => onGroupEdit(group.id)}
              >
                {t("rules.edit")}
              </button>
              <button
                type="button"
                className="text-button danger"
                onClick={() => onGroupRemove(group.id)}
              >
                {t("rules.delete")}
              </button>
            </span>
          </div>
          <div className="rc-desc">
            {group.students
              .map((id) => studentName(students, id))
              .join("、")}
            {" · "}
            {group.mode === "separate"
              ? t("groups.separate")
              : t("groups.together")}
          </div>
        </div>
      ))}

      <div className="rule-section-heading">
        <span className="chip chip-amber">{t("rules.soft")}</span>
        <span className="muted small">
          {t("rules.softHint")}
          <span className="rule-count">{softCount}</span>
        </span>
      </div>
      {softCount === 0 ? <p className="muted">{t("rules.softEmpty")}</p> : null}
      {PREFERENCE_META.filter((meta) => preferences.includes(meta.id)).map(
        (meta) => (
          <div className="rule-card rule-card-soft" key={meta.id}>
            <div className="rc-head">
              <label className="rc-toggle">
                <input
                  type="checkbox"
                  checked
                  onChange={() => onPreferenceToggle(meta.id)}
                />
                <span className="rc-title">{t(meta.label)}</span>
              </label>
              <span className="rc-actions">
                <button
                  type="button"
                  className="text-button danger"
                  onClick={() => onPreferenceToggle(meta.id)}
                >
                  {t("rules.delete")}
                </button>
              </span>
            </div>
            <div className="rc-desc">{t(meta.description)}</div>
          </div>
        ),
      )}
    </div>
  );
}
