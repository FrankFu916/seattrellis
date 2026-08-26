import { useEffect, useMemo, useState } from "react";

import { fetchRuleTemplates } from "../api/client";
import type {
  CatalogOption,
  CommonConstraint,
  CommonGroupRule,
  CommonPreferenceId,
  CompiledRule,
  DetailedRuleSettings,
  SentenceTemplate,
  Student,
} from "../api/types";
import {
  buildRulesSnapshot,
} from "../domain/generation";
import { compiledRuleTarget } from "../domain/ruleTemplates";
import type { Locale, MessageKey, Translate } from "../i18n/messages";
import { BulkConstraintEditor } from "./BulkConstraintEditor";
import { BulkGroupEditor } from "./BulkGroupEditor";
import { DetailedRulesPanel } from "./DetailedRulesPanel";
import { RuleCards } from "./RuleCards";
import { SentenceBuilder } from "./SentenceBuilder";

type RulesWorkbenchProps = {
  locale: Locale;
  t: Translate;
  students: Student[];
  seatIds: string[];
  goals: CatalogOption[];
  selectedGoalId: string;
  preferences: CommonPreferenceId[];
  constraints: CommonConstraint[];
  groups: CommonGroupRule[];
  detailedRules: DetailedRuleSettings;
  customRulesJson: string;
  onGoalChange: (goalId: string) => void;
  onPreferenceToggle: (id: CommonPreferenceId) => void;
  onConstraintAdd: () => void;
  onConstraintBatchAdd: (constraints: CommonConstraint[]) => void;
  onConstraintChange: (
    id: string,
    changes: Partial<CommonConstraint>,
  ) => void;
  onConstraintRemove: (id: string) => void;
  onGroupAdd: () => void;
  onGroupBatchAdd: (groups: CommonGroupRule[]) => void;
  onGroupChange: (id: string, changes: Partial<CommonGroupRule>) => void;
  onGroupRemove: (id: string) => void;
  onDetailedRulesChange: (changes: Partial<DetailedRuleSettings>) => void;
};

function optionName(
  option: { name: Record<Locale, string> },
  locale: Locale,
): string {
  return option.name[locale];
}

function optionDescription(
  option: { description: Record<Locale, string> },
  locale: Locale,
): string {
  return option.description[locale];
}

/**
 * The rules view (D3 fused form): goal presets on top, rule cards
 * (manage) + sentence builder (create) side by side, an advanced editor
 * for combinations the sentences cannot express, and a read-only JSON
 * view of the effective rule set (PD-D3-ADJ-1).
 */
export function RulesWorkbench({
  locale,
  t,
  students,
  seatIds,
  goals,
  selectedGoalId,
  preferences,
  constraints,
  groups,
  detailedRules,
  customRulesJson,
  onGoalChange,
  onPreferenceToggle,
  onConstraintAdd,
  onConstraintBatchAdd,
  onConstraintChange,
  onConstraintRemove,
  onGroupAdd,
  onGroupBatchAdd,
  onGroupChange,
  onGroupRemove,
  onDetailedRulesChange,
}: RulesWorkbenchProps) {
  const [templates, setTemplates] = useState<SentenceTemplate[] | null>(null);
  const [templatesError, setTemplatesError] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [jsonOpen, setJsonOpen] = useState(false);

  useEffect(() => {
    let current = true;
    void fetchRuleTemplates()
      .then((result) => {
        if (current) {
          setTemplates(result.templates);
          setTemplatesError(null);
        }
      })
      .catch((error: unknown) => {
        // The builder needs the local service; the cards still work. The
        // transport detail stays on the console.
        console.error("Rule templates could not be loaded", error);
        if (current) {
          setTemplates([]);
          setTemplatesError(t("rules.templatesUnavailable"));
        }
      });
    return () => {
      current = false;
    };
  }, [t]);

  const snapshot = useMemo(
    () =>
      buildRulesSnapshot({
        constraints,
        groups,
        preferences,
        detailedRules,
        customRulesJson,
        selectedGoalId,
      }),
    [constraints, groups, preferences, detailedRules, customRulesJson, selectedGoalId],
  );

  function retryTemplates() {
    setTemplates(null);
    setTemplatesError(null);
    fetchRuleTemplates()
      .then((result) => setTemplates(result.templates))
      .catch((error: unknown) => {
        console.error("Rule templates could not be loaded", error);
        setTemplates([]);
        setTemplatesError(t("rules.templatesUnavailable"));
      });
  }

  function handleAdd(compiled: CompiledRule) {
    const target = compiledRuleTarget(compiled);
    if (!target) {
      return;
    }
    switch (target.kind) {
      case "constraint":
        onConstraintBatchAdd([target.rule]);
        break;
      case "group":
        onGroupBatchAdd([target.rule]);
        break;
      case "preference":
        if (!preferences.includes(target.id)) {
          onPreferenceToggle(target.id);
        }
        break;
    }
  }

  return (
    <div className="rules-workbench">
      <fieldset className="choice-list">
        <legend className="sr-only">{t("step.goal.title")}</legend>
        {goals.map((goal, index) => (
          <label
            className="choice-card goal-choice"
            data-selected={goal.id === selectedGoalId}
            key={goal.id}
          >
            <input
              type="radio"
              name="goal"
              value={goal.id}
              checked={goal.id === selectedGoalId}
              onChange={() => onGoalChange(goal.id)}
            />
            <span
              className={`goal-symbol goal-symbol-${index + 1}`}
              aria-hidden="true"
            >
              {index === 0 ? "↻" : index === 1 ? "⌁" : "◇"}
            </span>
            <span className="choice-copy">
              <strong>{optionName(goal, locale)}</strong>
              <small>{optionDescription(goal, locale)}</small>
            </span>
            <span className="choice-check" aria-hidden="true">
              ✓
            </span>
          </label>
        ))}
      </fieldset>

      <div className="rules-workbench-grid">
        <div className="rules-cards-col">
          <RuleCards
            constraints={constraints}
            groups={groups}
            preferences={preferences}
            students={students}
            locale={locale}
            t={t}
            onConstraintToggle={(id, enabled) =>
              onConstraintChange(id, { enabled })
            }
            onConstraintRemove={onConstraintRemove}
            onConstraintEdit={() => setAdvancedOpen(true)}
            onGroupToggle={(id, enabled) => onGroupChange(id, { enabled })}
            onGroupRemove={onGroupRemove}
            onGroupEdit={() => setAdvancedOpen(true)}
            onPreferenceToggle={onPreferenceToggle}
          />
          <BulkConstraintEditor
            students={students}
            seatIds={seatIds}
            existingConstraints={constraints}
            t={t}
            onAdd={onConstraintBatchAdd}
          />
          <BulkGroupEditor
            students={students}
            existingGroups={groups}
            t={t}
            onAdd={onGroupBatchAdd}
          />
        </div>

        <div className="rules-builder-col">
          <SentenceBuilder
            templates={templates}
            templatesError={templatesError}
            students={students}
            seatIds={seatIds}
            locale={locale}
            t={t}
            onAdd={handleAdd}
            onRetryTemplates={retryTemplates}
          />
          <div className="rules-builder-tools">
            <button
              type="button"
              className="secondary-button"
              data-testid="advanced-toggle"
              onClick={() => setAdvancedOpen((open) => !open)}
            >
              {t("rules.advanced")}
              <span aria-hidden="true">{advancedOpen ? "▴" : "▾"}</span>
            </button>
            <button
              type="button"
              className="secondary-button"
              data-testid="json-toggle"
              onClick={() => setJsonOpen((open) => !open)}
            >
              {t("rules.viewJson")}
            </button>
          </div>
          {jsonOpen ? (
            <div className="rules-json-view" data-testid="rules-json-view">
              <pre className="json-view">{JSON.stringify(snapshot, null, 2)}</pre>
              <p className="note">{t("rules.jsonReadOnly")}</p>
            </div>
          ) : null}
        </div>
      </div>

      {advancedOpen ? (
        <div className="rules-advanced" data-testid="rules-advanced">
          <fieldset className="preference-list">
            <legend>{t("preference.title")}</legend>
            <p className="muted">{t("preference.hint")}</p>
            {PREFERENCE_ROWS.map((option) => (
              <label key={option.id} className="preference-row">
                <input
                  type="checkbox"
                  checked={preferences.includes(option.id)}
                  onChange={() => onPreferenceToggle(option.id)}
                />
                <span>
                  <strong>{t(option.label)}</strong>
                  <small>{t(option.description)}</small>
                </span>
              </label>
            ))}
          </fieldset>
          <section className="constraints-card" aria-labelledby="constraints-title">
            <div className="constraints-heading">
              <div>
                <h2 id="constraints-title">{t("constraints.title")}</h2>
                <p>{t("constraints.hint")}</p>
              </div>
              <button
                className="secondary-button"
                type="button"
                onClick={onConstraintAdd}
                disabled={students.length < 1}
              >
                {t("constraints.add")}
              </button>
            </div>
            {constraints.length === 0 ? (
              <p className="muted">{t("constraints.empty")}</p>
            ) : (
              <div className="constraint-list">
                {constraints.map((constraint) => (
                  <div className="constraint-row" key={constraint.id}>
                    <select
                      aria-label={t("constraints.type")}
                      value={constraint.kind}
                      onChange={(event) =>
                        onConstraintChange(constraint.id, {
                          kind: event.target.value as CommonConstraint["kind"],
                        })
                      }
                    >
                      <option value="avoid_adjacent">{t("constraints.avoidAdjacent")}</option>
                      <option value="must_adjacent">{t("constraints.mustAdjacent")}</option>
                      <option value="fixed_seat">{t("constraints.fixedSeat")}</option>
                      <option value="min_distance">{t("constraints.minDistance")}</option>
                    </select>
                    <select
                      aria-label={t("constraints.student")}
                      value={constraint.first}
                      onChange={(event) =>
                        onConstraintChange(constraint.id, { first: event.target.value })
                      }
                    >
                      <option value="">{t("constraints.chooseStudent")}</option>
                      {students.map((student) => (
                        <option key={student.id} value={student.id}>
                          {student.name} · {student.id}
                        </option>
                      ))}
                    </select>
                    {constraint.kind === "fixed_seat" ? (
                      <input
                        aria-label={t("constraints.seat")}
                        list="available-seat-ids"
                        value={constraint.seatId}
                        placeholder={t("constraints.seatPlaceholder")}
                        onChange={(event) =>
                          onConstraintChange(constraint.id, { seatId: event.target.value })
                        }
                      />
                    ) : (
                      <select
                        aria-label={t("constraints.otherStudent")}
                        value={constraint.second}
                        onChange={(event) =>
                          onConstraintChange(constraint.id, { second: event.target.value })
                        }
                      >
                        <option value="">{t("constraints.chooseStudent")}</option>
                        {students.map((student) => (
                          <option key={student.id} value={student.id}>
                            {student.name} · {student.id}
                          </option>
                        ))}
                      </select>
                    )}
                    {constraint.kind === "min_distance" ? (
                      <>
                        <label className="constraint-detail-field">
                          <span className="sr-only">{t("constraints.distance")}</span>
                          <input
                            type="number"
                            min={0.1}
                            step={0.1}
                            aria-label={t("constraints.distance")}
                            value={constraint.distance}
                            onChange={(event) =>
                              onConstraintChange(constraint.id, {
                                distance: Math.max(0.1, Number(event.target.value) || 0.1),
                              })
                            }
                          />
                        </label>
                        <select
                          aria-label={t("constraints.metric")}
                          value={constraint.metric}
                          onChange={(event) =>
                            onConstraintChange(constraint.id, {
                              metric: event.target.value as CommonConstraint["metric"],
                            })
                          }
                        >
                          <option value="graph">{t("constraints.metricGraph")}</option>
                          <option value="euclidean">{t("constraints.metricEuclidean")}</option>
                        </select>
                      </>
                    ) : null}
                    <button
                      className="icon-button"
                      type="button"
                      aria-label={t("constraints.remove")}
                      onClick={() => onConstraintRemove(constraint.id)}
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}
            <datalist id="available-seat-ids">
              {seatIds.map((seatId) => <option key={seatId} value={seatId} />)}
            </datalist>
          </section>
          <section className="constraints-card" aria-labelledby="groups-title">
            <div className="constraints-heading">
              <div>
                <h2 id="groups-title">{t("groups.title")}</h2>
                <p>{t("groups.hint")}</p>
              </div>
              <button
                className="secondary-button"
                type="button"
                onClick={onGroupAdd}
                disabled={students.length < 2}
              >
                {t("groups.add")}
              </button>
            </div>
            {groups.length === 0 ? (
              <p className="muted">{t("groups.empty")}</p>
            ) : (
              <div className="constraint-list">
                {groups.map((group) => (
                  <div className="constraint-row group-row" key={group.id}>
                    <input
                      aria-label={t("groups.name")}
                      value={group.name}
                      placeholder={t("groups.namePlaceholder")}
                      onChange={(event) =>
                        onGroupChange(group.id, { name: event.target.value })
                      }
                    />
                    <select
                      aria-label={t("groups.mode")}
                      value={group.mode}
                      onChange={(event) =>
                        onGroupChange(group.id, {
                          mode: event.target.value as CommonGroupRule["mode"],
                        })
                      }
                    >
                      <option value="separate">{t("groups.separate")}</option>
                      <option value="together">{t("groups.together")}</option>
                    </select>
                    <input
                      aria-label={t("groups.students")}
                      value={group.students.join(", ")}
                      placeholder={t("groups.studentsPlaceholder")}
                      onChange={(event) =>
                        onGroupChange(group.id, {
                          students: event.target.value
                            .split(/[,，\n]+/u)
                            .map((student) => student.trim())
                            .filter(Boolean),
                        })
                      }
                    />
                    <button
                      className="icon-button"
                      type="button"
                      aria-label={t("groups.remove")}
                      onClick={() => onGroupRemove(group.id)}
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}
            <small className="constraint-help">{t("groups.studentsHint")}</small>
          </section>
          <DetailedRulesPanel
            settings={detailedRules}
            t={t}
            onChange={onDetailedRulesChange}
          />
        </div>
      ) : null}
    </div>
  );
}

const PREFERENCE_ROWS: Array<{
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
