import type { ReactNode } from "react";

import type { Student } from "../api/types";
import type { MessageKey, Translate } from "../i18n/messages";

type JsonObject = Record<string, unknown>;
type HardListKey = "fixed_seats" | "must_be_adjacent" | "cannot_be_adjacent" | "min_distance";
type RuleRelation =
  | "desk_mate"
  | "horizontal"
  | "vertical"
  | "diagonal"
  | "adjacent_any"
  | "within_distance";

type RuleSetEditorPanelProps = {
  source: string;
  students: Student[];
  seatIds: string[];
  t: Translate;
  onChange: (source: string) => void;
};

const RULESET_SCHEMA_VERSION = 1;

const RELATIONS: RuleRelation[] = [
  "desk_mate",
  "horizontal",
  "vertical",
  "diagonal",
  "adjacent_any",
  "within_distance",
];

const ROTATION_CATEGORIES = [
  "front",
  "back",
  "side",
  "corner",
  "near_window",
  "near_door",
  "near_ac",
];

const SOFT_DEFAULTS: Record<string, JsonObject> = {
  vision_front: { enabled: true, weight: 20 },
  height_back: { enabled: true, weight: 1 },
  randomize: { enabled: true, weight: 1 },
  score_balance: { enabled: false, weight: 1 },
  score_position: { enabled: false, weight: 18, direction: "high_front" },
  score_distribution: { enabled: false, weight: 18, scope: "row" },
  mentor_pairing: {
    enabled: false,
    weight: 18,
    mentor_percentile: 0.75,
    learner_percentile: 0.25,
    relation: "desk_mate",
    avoid_recent_repeats: true,
    history_lookback: 4,
  },
  fair_rotation: {
    enabled: false,
    weight: 10,
    lookback: 4,
    avoid_repeating_categories: [...ROTATION_CATEGORIES],
  },
  avoid_recent_neighbors: {
    enabled: false,
    weight: 10,
    lookback: 4,
    max_recent_count: 1,
    within_distance: 2,
    relation_types: ["desk_mate", "adjacent_any"],
  },
  cooling: {
    enabled: false,
    weight: 5,
    cooling_period: 3,
    within_distance: 2,
    relation_types: ["desk_mate", "adjacent_any"],
  },
};

const SOFT_LABELS: Array<{ key: string; label: MessageKey }> = [
  { key: "vision_front", label: "preference.visionFront" },
  { key: "height_back", label: "preference.heightBack" },
  { key: "randomize", label: "ruleSetEditor.randomize" },
  { key: "score_balance", label: "ruleSetEditor.scoreBalance" },
  { key: "score_position", label: "preference.scorePosition" },
  { key: "score_distribution", label: "preference.scoreDistribution" },
  { key: "mentor_pairing", label: "preference.mentorPairing" },
  { key: "fair_rotation", label: "preference.fairRotation" },
  { key: "avoid_recent_neighbors", label: "preference.avoidNeighbors" },
  { key: "cooling", label: "detailedRules.cooling" },
];

function isObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function parseDocument(source: string): JsonObject | null {
  if (!source.trim()) return null;
  try {
    const value: unknown = JSON.parse(source);
    return isObject(value) ? value : null;
  } catch {
    return null;
  }
}

function defaultDocument(): JsonObject {
  return {
    schema_version: RULESET_SCHEMA_VERSION,
    seed: 42,
    hard: {
      fixed_seats: [],
      must_be_adjacent: [],
      cannot_be_adjacent: [],
      min_distance: [],
    },
    groups: [],
  };
}

function formatDocument(document: JsonObject): string {
  return `${JSON.stringify(document, null, 2)}\n`;
}

function listOfObjects(value: unknown): JsonObject[] {
  return Array.isArray(value) ? value.filter(isObject) : [];
}

function textValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function numberValue(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function integerValue(value: string, minimum: number, maximum: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return minimum;
  return Math.min(maximum, Math.max(minimum, Math.round(parsed)));
}

function relationLabelKey(relation: RuleRelation): MessageKey {
  const keys: Record<RuleRelation, MessageKey> = {
    desk_mate: "detailedRules.deskMate",
    horizontal: "detailedRules.horizontal",
    vertical: "detailedRules.vertical",
    diagonal: "detailedRules.diagonal",
    adjacent_any: "detailedRules.adjacentAny",
    within_distance: "detailedRules.withinDistanceRelation",
  };
  return keys[relation];
}

export function RuleSetEditorPanel({
  source,
  students,
  seatIds,
  t,
  onChange,
}: RuleSetEditorPanelProps) {
  const document = parseDocument(source);

  function write(next: JsonObject): void {
    onChange(formatDocument(next));
  }

  function createDocument(): void {
    write(defaultDocument());
  }

  if (!source.trim()) {
    return (
      <details className="ruleset-editor" data-testid="ruleset-editor">
        <summary>{t("ruleSetEditor.title")}</summary>
        <p className="advanced-settings-hint">{t("ruleSetEditor.hint")}</p>
        <div className="ruleset-editor-empty">
          <p>{t("ruleSetEditor.empty")}</p>
          <button
            className="secondary-button"
            type="button"
            data-testid="ruleset-editor-create"
            onClick={createDocument}
          >
            {t("ruleSetEditor.create")}
          </button>
        </div>
      </details>
    );
  }

  if (!document) {
    return (
      <details className="ruleset-editor" data-testid="ruleset-editor">
        <summary>{t("ruleSetEditor.title")}</summary>
        <p className="advanced-settings-hint">{t("ruleSetEditor.hint")}</p>
        <p className="ruleset-editor-invalid" role="alert">
          {t("ruleSetEditor.invalid")}
        </p>
      </details>
    );
  }

  const hard = isObject(document.hard) ? document.hard : {};
  const soft = isObject(document.soft) ? document.soft : {};
  const groups = listOfObjects(document.groups);

  function updateTopLevel(field: string, value: unknown): void {
    write({ ...document, [field]: value });
  }

  function updateHard(field: HardListKey, value: JsonObject[]): void {
    write({ ...document, hard: { ...hard, [field]: value } });
  }

  function updateSoft(field: string, changes: JsonObject): void {
    const current = isObject(soft[field]) ? soft[field] : SOFT_DEFAULTS[field] ?? {};
    write({
      ...document,
      soft: { ...soft, [field]: { ...current, ...changes } },
    });
  }

  function updateGroups(nextGroups: JsonObject[]): void {
    write({ ...document, groups: nextGroups });
  }

  function studentSelect(
    value: string,
    label: string,
    onSelect: (next: string) => void,
  ) {
    return (
      <select aria-label={label} value={value} onChange={(event) => onSelect(event.target.value)}>
        <option value="">{t("constraints.chooseStudent")}</option>
        {students.map((student) => (
          <option key={student.id} value={student.id}>
            {student.name} · {student.id}
          </option>
        ))}
      </select>
    );
  }

  function renderPairList(
    field: "must_be_adjacent" | "cannot_be_adjacent",
    title: MessageKey,
  ) {
    const rows = listOfObjects(hard[field]);
    return (
      <fieldset className="ruleset-editor-section">
        <legend>{t(title)}</legend>
        {rows.length === 0 ? <p className="muted">{t("ruleSetEditor.noRules")}</p> : null}
        <div className="ruleset-editor-list">
          {rows.map((row, index) => {
            const pair = Array.isArray(row.students) ? row.students : [];
            const first = textValue(pair[0]);
            const second = textValue(pair[1]);
            return (
              <div className="ruleset-editor-row" key={`${field}-${index}`}>
                {studentSelect(first, t("constraints.student"), (value) => {
                  const next = rows.map((item, itemIndex) =>
                    itemIndex === index
                      ? { ...item, students: [value, second] }
                      : item,
                  );
                  updateHard(field, next);
                })}
                {studentSelect(second, t("constraints.otherStudent"), (value) => {
                  const next = rows.map((item, itemIndex) =>
                    itemIndex === index
                      ? { ...item, students: [first, value] }
                      : item,
                  );
                  updateHard(field, next);
                })}
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t("ruleSetEditor.removeRule")}
                  onClick={() => updateHard(field, rows.filter((_, itemIndex) => itemIndex !== index))}
                >
                  ×
                </button>
              </div>
            );
          })}
        </div>
        <button
          className="text-button"
          type="button"
          data-testid={`ruleset-editor-add-${field}`}
          disabled={students.length < 2}
          onClick={() =>
            updateHard(field, [
              ...rows,
              { students: [students[0]?.id ?? "", students[1]?.id ?? ""] },
            ])
          }
        >
          {t("ruleSetEditor.addRule")}
        </button>
      </fieldset>
    );
  }

  function renderFixedSeats() {
    const rows = listOfObjects(hard.fixed_seats);
    return (
      <fieldset className="ruleset-editor-section">
        <legend>{t("constraints.fixedSeat")}</legend>
        {rows.length === 0 ? <p className="muted">{t("ruleSetEditor.noRules")}</p> : null}
        <div className="ruleset-editor-list">
          {rows.map((row, index) => (
            <div className="ruleset-editor-row" key={`fixed-${index}`}>
              {studentSelect(
                textValue(row.student),
                t("constraints.student"),
                (value) =>
                  updateHard(
                    "fixed_seats",
                    rows.map((item, itemIndex) =>
                      itemIndex === index ? { ...item, student: value } : item,
                    ),
                  ),
              )}
              <input
                aria-label={t("constraints.seat")}
                list="ruleset-editor-seats"
                value={textValue(row.seat_id)}
                placeholder={t("constraints.seatPlaceholder")}
                onChange={(event) =>
                  updateHard(
                    "fixed_seats",
                    rows.map((item, itemIndex) =>
                      itemIndex === index ? { ...item, seat_id: event.target.value } : item,
                    ),
                  )
                }
              />
              <button
                className="icon-button"
                type="button"
                aria-label={t("ruleSetEditor.removeRule")}
                onClick={() => updateHard("fixed_seats", rows.filter((_, itemIndex) => itemIndex !== index))}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        <button
          className="text-button"
          type="button"
          data-testid="ruleset-editor-add-fixed"
          disabled={students.length < 1 || seatIds.length < 1}
          onClick={() =>
            updateHard("fixed_seats", [
              ...rows,
              { student: students[0]?.id ?? "", seat_id: seatIds[0] ?? "" },
            ])
          }
        >
          {t("ruleSetEditor.addRule")}
        </button>
        <datalist id="ruleset-editor-seats">
          {seatIds.map((seatId) => <option key={seatId} value={seatId} />)}
        </datalist>
      </fieldset>
    );
  }

  function renderMinimumDistances() {
    const rows = listOfObjects(hard.min_distance);
    return (
      <fieldset className="ruleset-editor-section">
        <legend>{t("constraints.minDistance")}</legend>
        {rows.length === 0 ? <p className="muted">{t("ruleSetEditor.noRules")}</p> : null}
        <div className="ruleset-editor-list">
          {rows.map((row, index) => {
            const pair = Array.isArray(row.students) ? row.students : [];
            const first = textValue(pair[0]);
            const second = textValue(pair[1]);
            return (
              <div className="ruleset-editor-row ruleset-editor-distance" key={`distance-${index}`}>
                {studentSelect(first, t("constraints.student"), (value) =>
                  updateHard(
                    "min_distance",
                    rows.map((item, itemIndex) =>
                      itemIndex === index ? { ...item, students: [value, second] } : item,
                    ),
                  ),
                )}
                {studentSelect(second, t("constraints.otherStudent"), (value) =>
                  updateHard(
                    "min_distance",
                    rows.map((item, itemIndex) =>
                      itemIndex === index ? { ...item, students: [first, value] } : item,
                    ),
                  ),
                )}
                <input
                  aria-label={t("constraints.distance")}
                  type="number"
                  min={0.1}
                  step={0.1}
                  value={numberValue(row.distance, 1)}
                  onChange={(event) =>
                    updateHard(
                      "min_distance",
                      rows.map((item, itemIndex) =>
                        itemIndex === index
                          ? { ...item, distance: Math.max(0.1, Number(event.target.value) || 0.1) }
                          : item,
                      ),
                    )
                  }
                />
                <select
                  aria-label={t("constraints.metric")}
                  value={textValue(row.metric, "graph")}
                  onChange={(event) =>
                    updateHard(
                      "min_distance",
                      rows.map((item, itemIndex) =>
                        itemIndex === index ? { ...item, metric: event.target.value } : item,
                      ),
                    )
                  }
                >
                  <option value="graph">{t("constraints.metricGraph")}</option>
                  <option value="euclidean">{t("constraints.metricEuclidean")}</option>
                </select>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t("ruleSetEditor.removeRule")}
                  onClick={() => updateHard("min_distance", rows.filter((_, itemIndex) => itemIndex !== index))}
                >
                  ×
                </button>
              </div>
            );
          })}
        </div>
        <button
          className="text-button"
          type="button"
          data-testid="ruleset-editor-add-distance"
          disabled={students.length < 2}
          onClick={() =>
            updateHard("min_distance", [
              ...rows,
              {
                students: [students[0]?.id ?? "", students[1]?.id ?? ""],
                distance: 1,
                metric: "graph",
              },
            ])
          }
        >
          {t("ruleSetEditor.addRule")}
        </button>
      </fieldset>
    );
  }

  function renderRelations(rule: JsonObject, key: string): ReactNode {
    const selected = Array.isArray(rule.relation_types)
      ? rule.relation_types.filter((value): value is RuleRelation => RELATIONS.includes(value as RuleRelation))
      : [];
    return (
      <div className="rule-relations">
        <span className="rule-field-label">{t("detailedRules.relations")}</span>
        {RELATIONS.map((relation) => (
          <label key={relation}>
            <input
              type="checkbox"
              checked={selected.includes(relation)}
              onChange={(event) => {
                const next = event.target.checked
                  ? [...selected, relation]
                  : selected.filter((item) => item !== relation);
                updateSoft(key, {
                  relation_types: next.length ? next : [relation],
                });
              }}
            />
            {t(relationLabelKey(relation))}
          </label>
        ))}
      </div>
    );
  }

  function renderSoftRule(key: string, label: MessageKey) {
    const rule = { ...(SOFT_DEFAULTS[key] ?? {}), ...(isObject(soft[key]) ? soft[key] : {}) };
    const enabled = rule.enabled === true;
    return (
      <fieldset className="ruleset-editor-section ruleset-editor-soft" key={key}>
        <legend>{t(label)}</legend>
        <div className="ruleset-editor-soft-heading">
          <label className="rule-toggle">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(event) => updateSoft(key, { enabled: event.target.checked })}
            />
            <span>{t("ruleSetEditor.enabled")}</span>
          </label>
          <label className="advanced-field ruleset-editor-weight">
            <span>{t("detailedRules.weight")}</span>
            <input
              type="number"
              min={0}
              max={100}
              value={numberValue(rule.weight, 1)}
              onChange={(event) =>
                updateSoft(key, { weight: integerValue(event.target.value, 0, 100) })
              }
            />
          </label>
        </div>
        {key === "score_position" ? (
          <label className="advanced-field">
            <span>{t("detailedRules.direction")}</span>
            <select
              value={textValue(rule.direction, "high_front")}
              onChange={(event) => updateSoft(key, { direction: event.target.value })}
            >
              <option value="high_front">{t("detailedRules.highFront")}</option>
              <option value="high_back">{t("detailedRules.highBack")}</option>
            </select>
          </label>
        ) : null}
        {key === "score_distribution" ? (
          <label className="advanced-field">
            <span>{t("detailedRules.scope")}</span>
            <select
              value={textValue(rule.scope, "row")}
              onChange={(event) => updateSoft(key, { scope: event.target.value })}
            >
              <option value="row">{t("detailedRules.row")}</option>
              <option value="group">{t("detailedRules.group")}</option>
            </select>
          </label>
        ) : null}
        {key === "mentor_pairing" ? (
          <div className="rule-input-grid">
            <label className="advanced-field">
              <span>{t("detailedRules.mentorPercentile")}</span>
              <input
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={numberValue(rule.mentor_percentile, 0.75)}
                onChange={(event) => updateSoft(key, { mentor_percentile: Math.min(1, Math.max(0, Number(event.target.value) || 0)) })}
              />
            </label>
            <label className="advanced-field">
              <span>{t("detailedRules.learnerPercentile")}</span>
              <input
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={numberValue(rule.learner_percentile, 0.25)}
                onChange={(event) => updateSoft(key, { learner_percentile: Math.min(1, Math.max(0, Number(event.target.value) || 0)) })}
              />
            </label>
            <label className="advanced-field">
              <span>{t("detailedRules.relation")}</span>
              <select
                value={textValue(rule.relation, "desk_mate")}
                onChange={(event) => updateSoft(key, { relation: event.target.value })}
              >
                <option value="desk_mate">{t("detailedRules.deskMate")}</option>
                <option value="adjacent_any">{t("detailedRules.adjacentAny")}</option>
              </select>
            </label>
            <label className="advanced-field">
              <span>{t("detailedRules.historyLookback")}</span>
              <input
                type="number"
                min={0}
                max={100}
                value={numberValue(rule.history_lookback, 4)}
                onChange={(event) => updateSoft(key, { history_lookback: integerValue(event.target.value, 0, 100) })}
              />
            </label>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={rule.avoid_recent_repeats !== false}
                onChange={(event) => updateSoft(key, { avoid_recent_repeats: event.target.checked })}
              />
              <span>{t("detailedRules.avoidRecentRepeats")}</span>
            </label>
          </div>
        ) : null}
        {key === "fair_rotation" ? (
          <div className="rule-input-grid">
            <label className="advanced-field">
              <span>{t("detailedRules.lookback")}</span>
              <input
                type="number"
                min={0}
                max={100}
                value={numberValue(rule.lookback, 4)}
                onChange={(event) => updateSoft(key, { lookback: integerValue(event.target.value, 0, 100) })}
              />
            </label>
            <div className="rule-relations ruleset-editor-categories">
              <span className="rule-field-label">{t("ruleSetEditor.categories")}</span>
              {ROTATION_CATEGORIES.map((category) => {
                const selected = Array.isArray(rule.avoid_repeating_categories)
                  && rule.avoid_repeating_categories.includes(category);
                return (
                  <label key={category}>
                    <input
                      type="checkbox"
                      checked={selected}
                      onChange={(event) => {
                        const current = Array.isArray(rule.avoid_repeating_categories)
                          ? rule.avoid_repeating_categories.filter((value): value is string => typeof value === "string")
                          : [];
                        const next = event.target.checked
                          ? [...current, category]
                          : current.filter((value) => value !== category);
                        updateSoft(key, { avoid_repeating_categories: next.length ? next : [category] });
                      }}
                    />
                    <code>{category}</code>
                  </label>
                );
              })}
            </div>
          </div>
        ) : null}
        {key === "avoid_recent_neighbors" || key === "cooling" ? (
          <div className="rule-input-grid">
            <label className="advanced-field">
              <span>{key === "cooling" ? t("detailedRules.coolingPeriod") : t("detailedRules.lookback")}</span>
              <input
                type="number"
                min={key === "cooling" ? 1 : 0}
                max={100}
                value={numberValue(rule[key === "cooling" ? "cooling_period" : "lookback"], key === "cooling" ? 3 : 4)}
                onChange={(event) =>
                  updateSoft(key, {
                    [key === "cooling" ? "cooling_period" : "lookback"]: integerValue(event.target.value, key === "cooling" ? 1 : 0, 100),
                  })
                }
              />
            </label>
            <label className="advanced-field">
              <span>{t("detailedRules.withinDistance")}</span>
              <input
                type="number"
                min={1}
                max={20}
                value={numberValue(rule.within_distance, 2)}
                onChange={(event) => updateSoft(key, { within_distance: integerValue(event.target.value, 1, 20) })}
              />
            </label>
            {key === "avoid_recent_neighbors" ? (
              <label className="advanced-field">
                <span>{t("detailedRules.maxRecentCount")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={numberValue(rule.max_recent_count, 1)}
                  onChange={(event) => updateSoft(key, { max_recent_count: integerValue(event.target.value, 0, 100) })}
                />
              </label>
            ) : null}
            <div className="advanced-field-wide">
              {renderRelations(rule, key)}
            </div>
          </div>
        ) : null}
      </fieldset>
    );
  }

  function renderGroups() {
    return (
      <fieldset className="ruleset-editor-section">
        <legend>{t("groups.title")}</legend>
        {groups.length === 0 ? <p className="muted">{t("ruleSetEditor.noGroups")}</p> : null}
        <div className="ruleset-editor-list">
          {groups.map((group, index) => {
            const members = Array.isArray(group.students)
              ? group.students.filter((value): value is string => typeof value === "string")
              : [];
            const mode = group.together === true ? "together" : group.separate === true ? "separate" : "none";
            return (
              <div className="ruleset-editor-group" key={`group-${index}`}>
                <label className="advanced-field">
                  <span>{t("groups.name")}</span>
                  <input
                    value={textValue(group.name)}
                    onChange={(event) =>
                      updateGroups(groups.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))
                    }
                  />
                </label>
                <label className="advanced-field">
                  <span>{t("groups.mode")}</span>
                  <select
                    value={mode}
                    onChange={(event) => {
                      const nextMode = event.target.value;
                      updateGroups(groups.map((item, itemIndex) =>
                        itemIndex === index
                          ? { ...item, together: nextMode === "together", separate: nextMode === "separate" }
                          : item,
                      ));
                    }}
                  >
                    <option value="none">{t("ruleSetEditor.none")}</option>
                    <option value="together">{t("groups.together")}</option>
                    <option value="separate">{t("groups.separate")}</option>
                  </select>
                </label>
                <label className="advanced-field advanced-field-wide">
                  <span>{t("groups.students")}</span>
                  <input
                    value={members.join(", ")}
                    placeholder={t("groups.studentsPlaceholder")}
                    onChange={(event) =>
                      updateGroups(groups.map((item, itemIndex) =>
                        itemIndex === index
                          ? { ...item, students: event.target.value.split(/[,，\n]+/u).map((value) => value.trim()).filter(Boolean) }
                          : item,
                      ))
                    }
                  />
                </label>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t("groups.remove")}
                  onClick={() => updateGroups(groups.filter((_, itemIndex) => itemIndex !== index))}
                >
                  ×
                </button>
              </div>
            );
          })}
        </div>
        <button
          className="text-button"
          type="button"
          data-testid="ruleset-editor-add-group"
          onClick={() =>
            updateGroups([
              ...groups,
              {
                name: `Group ${groups.length + 1}`,
                students: students.slice(0, 2).map((student) => student.id),
                together: true,
                separate: false,
              },
            ])
          }
        >
          {t("ruleSetEditor.addGroup")}
        </button>
      </fieldset>
    );
  }

  return (
    <details className="ruleset-editor" data-testid="ruleset-editor">
      <summary>{t("ruleSetEditor.title")}</summary>
      <p className="advanced-settings-hint">{t("ruleSetEditor.hint")}</p>
      <div className="ruleset-editor-fields">
        <fieldset className="ruleset-editor-section ruleset-editor-metadata">
          <legend>{t("ruleSetEditor.metadata")}</legend>
          <label className="advanced-field">
            <span>{t("ruleSetEditor.schemaVersion")}</span>
            <input
              type="number"
              value={RULESET_SCHEMA_VERSION}
              readOnly
              aria-readonly="true"
            />
          </label>
          <label className="advanced-field">
            <span>{t("ruleSetEditor.seed")}</span>
            <input
              type="number"
              value={numberValue(document.seed, 42)}
              onChange={(event) => updateTopLevel("seed", integerValue(event.target.value, -2147483648, 2147483647))}
            />
          </label>
        </fieldset>

        <div className="ruleset-editor-section-title">{t("ruleSetEditor.hard")}</div>
        {renderFixedSeats()}
        {renderPairList("must_be_adjacent", "constraints.mustAdjacent")}
        {renderPairList("cannot_be_adjacent", "constraints.avoidAdjacent")}
        {renderMinimumDistances()}

        <div className="ruleset-editor-section-title">{t("ruleSetEditor.soft")}</div>
        {SOFT_LABELS.map(({ key, label }) => renderSoftRule(key, label))}

        <div className="ruleset-editor-section-title">{t("ruleSetEditor.groups")}</div>
        {renderGroups()}
      </div>
    </details>
  );
}
