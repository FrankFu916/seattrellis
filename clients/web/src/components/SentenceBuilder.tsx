import { useEffect, useMemo, useState } from "react";

import { compileRuleSentence } from "../api/client";
import type {
  BilingualText,
  CompiledRule,
  RuleCompileError,
  SentenceSlot,
  SentenceTemplate,
  Student,
} from "../api/types";
import type { Locale, Translate } from "../i18n/messages";
import { describeApiError } from "../domain/errorMessages";

type SentenceBuilderProps = {
  templates: SentenceTemplate[] | null;
  templatesError?: string | null;
  students: Student[];
  seatIds: string[];
  locale: Locale;
  t: Translate;
  onAdd: (compiled: CompiledRule) => void;
  onRetryTemplates?: () => void;
};

function localized(text: BilingualText, locale: Locale): string {
  return text[locale === "zh-CN" ? "zh" : "en"];
}

/** Split a sentence into text and slot segments on `{slot}` placeholders. */
export function splitSentence(
  sentence: string,
): Array<{ text?: string; slot?: string }> {
  const parts: Array<{ text?: string; slot?: string }> = [];
  const pattern = /\{(\w+)\}/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(sentence)) !== null) {
    if (match.index > last) {
      parts.push({ text: sentence.slice(last, match.index) });
    }
    parts.push({ slot: match[1] });
    last = match.index + match[0].length;
  }
  if (last < sentence.length) {
    parts.push({ text: sentence.slice(last) });
  }
  return parts;
}

function studentName(students: Student[], id: string): string {
  return students.find((student) => student.id === id)?.name ?? id;
}

/**
 * The sentence builder (D3): a template sentence with clickable slots.
 * Filling every required slot enables "add to rules"; the compile request
 * goes to the Rust service, so the added rule is always a Rust artifact.
 */
export function SentenceBuilder({
  templates,
  templatesError = null,
  students,
  seatIds,
  locale,
  t,
  onAdd,
  onRetryTemplates,
}: SentenceBuilderProps) {
  const [activeId, setActiveId] = useState<string | null>(null);
  const [slotValues, setSlotValues] = useState<Record<string, string>>({});
  const [activeSlot, setActiveSlot] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const template = templates?.find((item) => item.id === activeId) ?? null;

  useEffect(() => {
    if (templates && templates.length > 0) {
      setActiveId((current) =>
        current && templates.some((item) => item.id === current)
          ? current
          : templates[0].id,
      );
    }
  }, [templates]);

  const filled = useMemo(() => {
    if (!template) {
      return 0;
    }
    return template.slots.filter((slot) =>
      slotValues[slot.key]?.trim() ? true : false,
    ).length;
  }, [template, slotValues]);

  if (!templates || templates.length === 0) {
    return (
      <div className="sentence-builder">
        <p className="muted">{t("rules.templatesUnavailable")}</p>
        {templatesError ? (
          <p className="inline-error" role="alert" data-testid="templates-error">
            {templatesError}
          </p>
        ) : null}
        {onRetryTemplates ? (
          <button
            type="button"
            className="secondary-button"
            onClick={onRetryTemplates}
          >
            {t("rules.retryTemplates")}
          </button>
        ) : null}
      </div>
    );
  }
  if (!template) {
    return null;
  }

  const requiredCount = template.slots.filter((slot) => slot.required).length;
  const complete = filled >= requiredCount;
  const activeSlotSpec = template.slots.find(
    (slot) => slot.key === activeSlot,
  );

  function selectTemplate(id: string) {
    setActiveId(id);
    setSlotValues({});
    setActiveSlot(null);
    setError(null);
  }

  function slotLabel(slot: SentenceSlot): string {
    const value = slotValues[slot.key];
    if (!value?.trim()) {
      return slot.placeholder
        ? localized(slot.placeholder, locale)
        : t("rules.slotChoose");
    }
    switch (slot.kind) {
      case "student":
        return studentName(students, value);
      case "students":
        return value
          .split(",")
          .map((id) => studentName(students, id.trim()))
          .join("、");
      case "seat":
        return value;
      case "choice": {
        const option = slot.options?.find((item) => item.value === value);
        return option ? localized(option.label, locale) : value;
      }
      default:
        return value;
    }
  }

  async function addRule() {
    if (!template || !complete || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // Number slots travel as numbers; everything else as strings.
      const payload: Record<string, string | number> = {};
      for (const [key, value] of Object.entries(slotValues)) {
        const slot = template.slots.find((item) => item.key === key);
        payload[key] =
          slot?.kind === "number" && value.trim() !== ""
            ? Number(value)
            : value;
      }
      const compiled = await compileRuleSentence(template.id, payload);
      onAdd(compiled);
      setSlotValues({});
      setActiveSlot(null);
    } catch (err) {
      const detail = err as { code?: string; slot?: string | null; message?: string };
      // Map the stable compile-error codes onto teacher-facing sentences; the
      // raw transport message never reaches the panel (W7 pattern).
      const localizedMessage =
        detail.code === "missing_slot"
          ? t("rules.compileMissingSlot")
          : detail.code === "invalid_choice"
            ? t("rules.compileInvalidChoice")
            : detail.code === "unknown_template"
              ? t("rules.compileFailedGeneric")
              : describeApiError(err, t, "rules.compileFailedGeneric");
      setError(t("rules.compileFailed", { message: localizedMessage }));
      if (detail.slot) {
        setActiveSlot(detail.slot);
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="sentence-builder">
      <div className="rules-builder-heading">
        <strong>{t("rules.newRule")}</strong>
        <span className="chip blue">{t("rules.templateChips")}</span>
      </div>
      <div className="template-chips" aria-label={t("rules.templateChips")}>
        {templates.map((item) => (
          <button
            type="button"
            key={item.id}
            className="template-chip"
            data-active={item.id === template.id}
            onClick={() => selectTemplate(item.id)}
          >
            {localized(item.label, locale)}
          </button>
        ))}
      </div>
      <div className="rule-sentence" aria-label={t("rules.sentenceLabel")}>
        <span
          className={`chip ${
            template.category === "hard" ? "chip-red" : "chip-amber"
          }`}
        >
          {template.category === "hard" ? t("rules.hard") : t("rules.soft")}
        </span>
        {splitSentence(localized(template.sentence, locale)).map(
          (part, index) => {
            if (part.slot) {
              const slot = template.slots.find(
                (item) => item.key === part.slot,
              );
              if (!slot) {
                return null;
              }
              const value = slotValues[slot.key];
              return (
                <button
                  type="button"
                  key={`${part.slot}-${index}`}
                  className="sentence-slot"
                  data-filled={Boolean(value?.trim())}
                  data-active={activeSlot === slot.key}
                  onClick={() =>
                    setActiveSlot((current) =>
                      current === slot.key ? null : slot.key,
                    )
                  }
                >
                  {slotLabel(slot)}
                </button>
              );
            }
            return (
              <span className="seg" key={index}>
                {part.text}
              </span>
            );
          },
        )}
      </div>

      {activeSlotSpec ? (
        <SlotEditor
          slot={activeSlotSpec}
          students={students}
          seatIds={seatIds}
          locale={locale}
          t={t}
          value={slotValues[activeSlotSpec.key] ?? ""}
          onChange={(value) =>
            setSlotValues((current) => ({ ...current, [activeSlotSpec.key]: value }))
          }
          onDone={() => setActiveSlot(null)}
        />
      ) : null}

      <div className="rules-builder-actions">
        <span className={`compile-status ${complete ? "ok" : ""}`}>
          {complete
            ? t("rules.compileReady")
            : t("rules.compilePending", {
                filled,
                required: requiredCount,
              })}
        </span>
        <button
          type="button"
          className="primary-button"
          disabled={!complete || busy}
          onClick={() => void addRule()}
        >
          {busy ? t("action.generating") : t("rules.addRule")}
        </button>
      </div>
      {error ? (
        <p className="inline-error" role="alert">
          {error}
        </p>
      ) : null}
      <p className="preview-line">
        {complete
          ? t("rules.compilePreview")
          : t("rules.compileIncomplete")}
      </p>
    </div>
  );
}

type SlotEditorProps = {
  slot: SentenceSlot;
  students: Student[];
  seatIds: string[];
  locale: Locale;
  t: Translate;
  value: string;
  onChange: (value: string) => void;
  onDone: () => void;
};

function SlotEditor({
  slot,
  students,
  seatIds,
  locale,
  t,
  value,
  onChange,
  onDone,
}: SlotEditorProps) {
  return (
    <div className="slot-editor" data-testid={`slot-editor-${slot.key}`}>
      <span className="slot-editor-label">{localized(slot.label, locale)}</span>
      {slot.kind === "student" ? (
        <select
          aria-label={localized(slot.label, locale)}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">{t("rules.slotChoose")}</option>
          {students.map((student) => (
            <option key={student.id} value={student.id}>
              {student.name} · {student.id}
            </option>
          ))}
        </select>
      ) : slot.kind === "students" ? (
        <div className="slot-students">
          {students.map((student) => (
            <label key={student.id}>
              <input
                type="checkbox"
                checked={value
                  .split(",")
                  .map((id) => id.trim())
                  .includes(student.id)}
                onChange={(event) => {
                  const ids = value
                    .split(",")
                    .map((id) => id.trim())
                    .filter(Boolean);
                  const next = event.target.checked
                    ? [...ids, student.id]
                    : ids.filter((id) => id !== student.id);
                  onChange(next.join(", "));
                }}
              />
              {student.name} · {student.id}
            </label>
          ))}
        </div>
      ) : slot.kind === "seat" ? (
        <select
          aria-label={localized(slot.label, locale)}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">{t("rules.slotChoose")}</option>
          {seatIds.map((seatId) => (
            <option key={seatId} value={seatId}>
              {seatId}
            </option>
          ))}
        </select>
      ) : slot.kind === "choice" ? (
        <select
          aria-label={localized(slot.label, locale)}
          value={value || String(slot.default ?? "")}
          onChange={(event) => onChange(event.target.value)}
        >
          {slot.options?.map((option) => (
            <option key={option.value} value={option.value}>
              {localized(option.label, locale)}
            </option>
          ))}
        </select>
      ) : slot.kind === "number" ? (
        <input
          type="number"
          aria-label={localized(slot.label, locale)}
          min={slot.min ?? undefined}
          max={slot.max ?? undefined}
          step={slot.step ?? undefined}
          value={value}
          placeholder={String(slot.default ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <input
          type="text"
          aria-label={localized(slot.label, locale)}
          value={value}
          placeholder={
            slot.placeholder
              ? localized(slot.placeholder, locale)
              : localized(slot.label, locale)
          }
          onChange={(event) => onChange(event.target.value)}
        />
      )}
      <button type="button" className="text-button" onClick={onDone}>
        {t("rules.slotDone")}
      </button>
    </div>
  );
}
