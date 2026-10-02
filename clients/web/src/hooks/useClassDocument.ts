import { useEffect, useRef, useState } from "react";
import {
  fetchEditorState,
  openClassDocument,
  serializeClassDocument,
} from "../api/client";
import type {
  ClassDocument,
  ClassSource,
  EditorState,
  OpenClassDocumentResponse,
  RotationPlan,
} from "../api/types";
import {
  chooseClassSaveTarget,
  writeClassFile,
  type ClassSaveTarget,
} from "../domain/classFiles";
import { readClassSource } from "../domain/classSource";
import { isTauriDesktop, pickFileWithDialog } from "../domain/desktop";
import { newCommandId } from "../domain/identifiers";
import type { ClassContext } from "../domain/navigation";
import type { Translate } from "../i18n/messages";
import type { DraftVersion } from "./useClassRevisions";

interface SavedClass {
  document: ClassDocument;
  target: ClassSaveTarget | null;
}

interface ClassDocumentOptions {
  t: Translate;
  context: ClassContext;
  isDirty: boolean;
  captureSource: (name?: string) => ClassSource;
  captureDrafts: () => DraftVersion[];
  scratchRevision: number;
  rotationPlan: RotationPlan | null;
  captureWorkspaceOwner: () => () => boolean;
  releaseDrafts: (ids: string[]) => void;
  isRevisionConflict: (error: unknown) => boolean;
  onError: (error: unknown) => void;
  onClearError: () => void;
  onWritten: () => void;
  onSaved: (
    source: ClassSource,
    drafts: DraftVersion[],
    scratchRevision: number,
    context: ClassContext,
  ) => void;
  onOpened: (
    result: OpenClassDocumentResponse,
    source: ClassSource,
    candidateEditors: EditorState[],
    context: ClassContext,
  ) => void;
  onRefreshedEditors: (editors: EditorState[]) => void;
}

function openedDraftIds(result: OpenClassDocumentResponse): string[] {
  return [...new Set(
    [
      result.editor?.draft_id,
      ...result.period_editors.map((editor) => editor.draft_id),
      ...result.candidates.map((candidate) => candidate.candidate_id),
    ].filter((id): id is string => !!id),
  )];
}

/** Owns file permissions, saved session documents and asynchronous open/save work. */
export function useClassDocument(options: ClassDocumentOptions) {
  const operationRef = useRef(0);
  const targetRef = useRef<ClassSaveTarget | null>(null);
  const savedClassesRef = useRef(new Map<string, SavedClass>());
  const inputRef = useRef<HTMLInputElement>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => () => { operationRef.current += 1; }, []);

  function captureOwner() {
    const token = operationRef.current;
    return () => token === operationRef.current;
  }

  function reset() {
    operationRef.current += 1;
    targetRef.current = null;
    setStatus(null);
    setIsSaving(false);
  }

  async function save(name?: string, saveAs = false) {
    const token = ++operationRef.current;
    const isCurrent = () => token === operationRef.current;
    const source = options.captureSource(name);
    const drafts = options.captureDrafts();
    const scratchRevision = options.scratchRevision;
    const filename = `${source.name.replace(/[\\/:*?"<>|]/g, "_")}.seattrellis.json`;
    setIsSaving(true);
    options.onClearError();
    try {
      // The picker must start in the user activation that invoked Save.
      const target = await chooseClassSaveTarget(
        filename,
        saveAs ? null : targetRef.current,
      );
      if (!target || !isCurrent()) return;
      const document = await serializeClassDocument(source, drafts, options.rotationPlan);
      if (!isCurrent()) return;
      const outcome = await writeClassFile(
        target,
        filename,
        new Blob([JSON.stringify(document, null, 2)], { type: "application/json" }),
      );
      if (!isCurrent()) return;
      targetRef.current = target;
      setStatus(options.t(outcome === "saved" ? "classFile.saved" : "classFile.downloaded"));
      options.onWritten();
      if (outcome === "saved") {
        const id = saveAs || options.context.kind === "temp"
          ? newCommandId()
          : options.context.id;
        const context: ClassContext = { kind: "class", id, name: source.name };
        savedClassesRef.current.set(id, { document, target });
        options.onSaved(source, drafts, scratchRevision, context);
      }
    } catch (error) {
      if (!isCurrent()) return;
      options.onError(error);
      if (options.isRevisionConflict(error)) {
        const results = await Promise.allSettled(
          drafts.map(({ draft_id }) => fetchEditorState(draft_id)),
        );
        if (isCurrent()) {
          options.onRefreshedEditors(results.flatMap((result) =>
            result.status === "fulfilled" ? [result.value] : [],
          ));
        }
      }
    } finally {
      if (isCurrent()) setIsSaving(false);
    }
  }

  async function restore(
    document: unknown,
    target: ClassSaveTarget | null = null,
    context?: ClassContext,
  ) {
    const token = ++operationRef.current;
    const isCurrent = () => token === operationRef.current;
    let ownedDrafts: string[] = [];
    setIsSaving(true);
    options.onClearError();
    try {
      const result = await openClassDocument(document);
      ownedDrafts = openedDraftIds(result);
      if (!isCurrent()) return;
      const source = readClassSource(result.class_source);
      const candidateEditors = await Promise.all(
        result.candidates.map((candidate) => fetchEditorState(candidate.candidate_id)),
      );
      if (!isCurrent()) return;
      const next = context ?? { kind: "class" as const, id: newCommandId(), name: source.name };
      options.onOpened(result, source, candidateEditors, next);
      // Reading file bytes establishes a saved baseline, but does not grant
      // write access. A null target makes the next Save choose a destination.
      if (next.kind === "class") {
        savedClassesRef.current.set(next.id, {
          document: { ...(document as ClassDocument), class_source: source },
          target,
        });
      }
      ownedDrafts = [];
      targetRef.current = target;
      setStatus(options.t("classFile.opened"));
    } catch (error) {
      if (isCurrent()) options.onError(error);
    } finally {
      options.releaseDrafts(ownedDrafts);
      if (isCurrent()) setIsSaving(false);
    }
  }

  async function open(file?: File) {
    const ownsDocument = captureOwner();
    const ownsWorkspace = options.captureWorkspaceOwner();
    try {
      if (!file) {
        if (isTauriDesktop()) {
          file = (await pickFileWithDialog(["json"], options.t("classFile.open"))) ?? undefined;
        } else {
          inputRef.current?.click();
          return;
        }
      }
      if (!file || !ownsDocument()) return;
      if (options.isDirty && !window.confirm(options.t("app.discardDraft"))) return;
      if (file.size > 20 * 1024 * 1024) throw new Error("Class file too large");
      const document = JSON.parse(await file.text());
      if (!ownsDocument() || !ownsWorkspace()) return;
      await restore(document);
    } catch (error) {
      if (ownsDocument() && ownsWorkspace()) options.onError(error);
    }
  }

  return {
    inputRef, isSaving, status, save, restore, open, reset, captureOwner,
    clearStatus: () => setStatus(null),
    findSaved: (id: string) => savedClassesRef.current.get(id),
  };
}
