import type { DetailedRuleSettings, RuleRelation } from "../api/types";
import type { MessageKey, Translate } from "../i18n/messages";

type DetailedRulesPanelProps = {
  settings: DetailedRuleSettings;
  t: Translate;
  onChange: (changes: Partial<DetailedRuleSettings>) => void;
};

const RELATIONS: RuleRelation[] = [
  "desk_mate",
  "horizontal",
  "vertical",
  "diagonal",
  "adjacent_any",
  "within_distance",
];
type RuleSection = Exclude<keyof DetailedRuleSettings, "enabled">;

function boundedInteger(value: string, minimum: number, maximum: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return minimum;
  }
  return Math.min(maximum, Math.max(minimum, Math.round(parsed)));
}

function boundedPercentile(value: string): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return 0;
  }
  return Math.min(1, Math.max(0, parsed));
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

export function DetailedRulesPanel({
  settings,
  t,
  onChange,
}: DetailedRulesPanelProps) {
  const updateSection = <K extends RuleSection>(
    section: K,
    changes: Partial<DetailedRuleSettings[K]>,
  ) => {
    onChange({
      [section]: {
        ...settings[section],
        ...changes,
      },
    } as Partial<DetailedRuleSettings>);
  };

  const toggleRelation = (
    section: "avoidRecentNeighbors" | "cooling",
    relation: RuleRelation,
  ) => {
    const current = settings[section].relationTypes;
    if (current.includes(relation)) {
      // The backend needs at least one relation to inspect. Keep the last
      // selected relation instead of silently creating an inactive rule.
      if (current.length === 1) {
        return;
      }
      updateSection(section, {
        relationTypes: current.filter((item) => item !== relation),
      });
      return;
    }
    updateSection(section, {
      relationTypes: [...current, relation],
    });
  };

  return (
    <details
      className="detailed-rules-settings"
      data-testid="detailed-rules-settings"
      open={settings.enabled}
    >
      <summary>{t("detailedRules.title")}</summary>
      <p className="advanced-settings-hint">{t("detailedRules.hint")}</p>
      <label className="detailed-rules-toggle">
        <input
          data-testid="detailed-rules-toggle"
          type="checkbox"
          checked={settings.enabled}
          onChange={(event) => onChange({ enabled: event.target.checked })}
        />
        <span>{t("detailedRules.enabled")}</span>
      </label>

      {settings.enabled ? (
        <div className="detailed-rules-fields">
          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.fairRotation")}</legend>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.fairRotation.enabled}
                onChange={(event) =>
                  updateSection("fairRotation", { enabled: event.target.checked })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.fairRotation.weight}
                  onChange={(event) =>
                    updateSection("fairRotation", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.lookback")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.fairRotation.lookback}
                  onChange={(event) =>
                    updateSection("fairRotation", {
                      lookback: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
            </div>
          </fieldset>

          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.recentNeighbors")}</legend>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.avoidRecentNeighbors.enabled}
                onChange={(event) =>
                  updateSection("avoidRecentNeighbors", {
                    enabled: event.target.checked,
                  })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.avoidRecentNeighbors.weight}
                  onChange={(event) =>
                    updateSection("avoidRecentNeighbors", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.lookback")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.avoidRecentNeighbors.lookback}
                  onChange={(event) =>
                    updateSection("avoidRecentNeighbors", {
                      lookback: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.maxRecentCount")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.avoidRecentNeighbors.maxRecentCount}
                  onChange={(event) =>
                    updateSection("avoidRecentNeighbors", {
                      maxRecentCount: boundedInteger(event.target.value, 0, 100),
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
                  value={settings.avoidRecentNeighbors.withinDistance}
                  onChange={(event) =>
                    updateSection("avoidRecentNeighbors", {
                      withinDistance: boundedInteger(event.target.value, 1, 20),
                    })
                  }
                />
              </label>
            </div>
            <div className="rule-relations">
              <span className="rule-field-label">{t("detailedRules.relations")}</span>
              {RELATIONS.map((relation) => (
                <label key={relation}>
                  <input
                    type="checkbox"
                    checked={settings.avoidRecentNeighbors.relationTypes.includes(relation)}
                  onChange={() => toggleRelation("avoidRecentNeighbors", relation)}
                  />
                  {t(relationLabelKey(relation))}
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.cooling")}</legend>
            <p className="rule-help">{t("detailedRules.coolingHint")}</p>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.cooling.enabled}
                onChange={(event) =>
                  updateSection("cooling", { enabled: event.target.checked })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.cooling.weight}
                  onChange={(event) =>
                    updateSection("cooling", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.coolingPeriod")}</span>
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={settings.cooling.coolingPeriod}
                  onChange={(event) =>
                    updateSection("cooling", {
                      coolingPeriod: boundedInteger(event.target.value, 1, 100),
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
                  value={settings.cooling.withinDistance}
                  onChange={(event) =>
                    updateSection("cooling", {
                      withinDistance: boundedInteger(event.target.value, 1, 20),
                    })
                  }
                />
              </label>
            </div>
            <div className="rule-relations">
              <span className="rule-field-label">{t("detailedRules.relations")}</span>
              {RELATIONS.map((relation) => (
                <label key={relation}>
                  <input
                    type="checkbox"
                    checked={settings.cooling.relationTypes.includes(relation)}
                    onChange={() => toggleRelation("cooling", relation)}
                  />
                  {t(relationLabelKey(relation))}
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.scorePosition")}</legend>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.scorePosition.enabled}
                onChange={(event) =>
                  updateSection("scorePosition", { enabled: event.target.checked })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.scorePosition.weight}
                  onChange={(event) =>
                    updateSection("scorePosition", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.direction")}</span>
                <select
                  value={settings.scorePosition.direction}
                  onChange={(event) =>
                    updateSection("scorePosition", {
                      direction: event.target.value as "high_front" | "high_back",
                    })
                  }
                >
                  <option value="high_front">{t("detailedRules.highFront")}</option>
                  <option value="high_back">{t("detailedRules.highBack")}</option>
                </select>
              </label>
            </div>
          </fieldset>

          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.scoreDistribution")}</legend>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.scoreDistribution.enabled}
                onChange={(event) =>
                  updateSection("scoreDistribution", { enabled: event.target.checked })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.scoreDistribution.weight}
                  onChange={(event) =>
                    updateSection("scoreDistribution", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.scope")}</span>
                <select
                  value={settings.scoreDistribution.scope}
                  onChange={(event) =>
                    updateSection("scoreDistribution", {
                      scope: event.target.value as "row" | "group",
                    })
                  }
                >
                  <option value="row">{t("detailedRules.row")}</option>
                  <option value="group">{t("detailedRules.group")}</option>
                </select>
              </label>
            </div>
            <small className="rule-help">{t("detailedRules.groupHint")}</small>
          </fieldset>

          <fieldset className="detailed-rule-card">
            <legend>{t("detailedRules.mentorPairing")}</legend>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.mentorPairing.enabled}
                onChange={(event) =>
                  updateSection("mentorPairing", { enabled: event.target.checked })
                }
              />
              <span>{t("detailedRules.enabledLabel")}</span>
            </label>
            <div className="rule-input-grid">
              <label className="advanced-field">
                <span>{t("detailedRules.weight")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.mentorPairing.weight}
                  onChange={(event) =>
                    updateSection("mentorPairing", {
                      weight: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.relation")}</span>
                <select
                  value={settings.mentorPairing.relation}
                  onChange={(event) =>
                    updateSection("mentorPairing", {
                      relation: event.target.value as RuleRelation,
                    })
                  }
                >
                  <option value="desk_mate">{t("detailedRules.deskMate")}</option>
                  <option value="adjacent_any">{t("detailedRules.adjacentAny")}</option>
                </select>
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.mentorPercentile")}</span>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={settings.mentorPairing.mentorPercentile}
                  onChange={(event) =>
                    updateSection("mentorPairing", {
                      mentorPercentile: boundedPercentile(event.target.value),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.learnerPercentile")}</span>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={settings.mentorPairing.learnerPercentile}
                  onChange={(event) =>
                    updateSection("mentorPairing", {
                      learnerPercentile: boundedPercentile(event.target.value),
                    })
                  }
                />
              </label>
              <label className="advanced-field">
                <span>{t("detailedRules.historyLookback")}</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={settings.mentorPairing.historyLookback}
                  onChange={(event) =>
                    updateSection("mentorPairing", {
                      historyLookback: boundedInteger(event.target.value, 0, 100),
                    })
                  }
                />
              </label>
            </div>
            <label className="rule-toggle">
              <input
                type="checkbox"
                checked={settings.mentorPairing.avoidRecentRepeats}
                onChange={(event) =>
                  updateSection("mentorPairing", {
                    avoidRecentRepeats: event.target.checked,
                  })
                }
              />
              <span>{t("detailedRules.avoidRecentRepeats")}</span>
            </label>
            <small className="rule-help">{t("detailedRules.percentileHint")}</small>
          </fieldset>

          <p className="detailed-rules-note">{t("detailedRules.compatibilityNote")}</p>
        </div>
      ) : null}
    </details>
  );
}
