import { useEffect, useReducer } from "react";

export interface DraftVersion {
  draft_id: string;
  revision: number;
}

interface RevisionLedger {
  sourceRevision: number;
  savedSourceRevision: number;
  generatedSourceRevision: number | null;
  draftRevisions: Record<string, number>;
  savedDraftRevisions: Record<string, number>;
  scratchRevision: number;
  savedScratchRevision: number;
}

const initialLedger: RevisionLedger = {
  sourceRevision: 0,
  savedSourceRevision: 0,
  generatedSourceRevision: null,
  draftRevisions: {},
  savedDraftRevisions: {},
  scratchRevision: 0,
  savedScratchRevision: 0,
};

type RevisionAction =
  | { type: "source_changed" }
  | { type: "scratch_changed" }
  | { type: "scratch_cleared" }
  | { type: "remember_draft"; id: string; revision: number }
  | { type: "forget_drafts"; ids: string[] }
  | { type: "replace_drafts"; versions: Record<string, number> }
  | { type: "generated_source"; revision: number | null }
  | {
      type: "saved";
      sourceRevision: number;
      drafts: DraftVersion[];
      scratchRevision: number;
    }
  | {
      type: "opened";
      sourceRevision: number;
      generatedSourceRevision: number | null;
      versions: Record<string, number>;
    }
  | { type: "reset" };

function revisionReducer(state: RevisionLedger, action: RevisionAction): RevisionLedger {
  switch (action.type) {
    case "source_changed":
      return { ...state, sourceRevision: state.sourceRevision + 1 };
    case "scratch_changed":
      return { ...state, scratchRevision: state.scratchRevision + 1 };
    case "scratch_cleared":
      return { ...state, savedScratchRevision: state.scratchRevision };
    case "remember_draft":
      return {
        ...state,
        draftRevisions: { ...state.draftRevisions, [action.id]: action.revision },
      };
    case "forget_drafts": {
      const keep = ([id]: [string, number]) => !action.ids.includes(id);
      return {
        ...state,
        draftRevisions: Object.fromEntries(Object.entries(state.draftRevisions).filter(keep)),
        savedDraftRevisions: Object.fromEntries(Object.entries(state.savedDraftRevisions).filter(keep)),
      };
    }
    case "replace_drafts":
      return { ...state, draftRevisions: action.versions };
    case "generated_source":
      return { ...state, generatedSourceRevision: action.revision };
    case "saved":
      // Only the captured versions become the saved baseline. Edits that
      // happened while a dialog or file write was pending remain dirty.
      return {
        ...state,
        savedSourceRevision: action.sourceRevision,
        savedDraftRevisions: Object.fromEntries(
          action.drafts.map(({ draft_id, revision }) => [draft_id, revision]),
        ),
        savedScratchRevision: action.scratchRevision,
      };
    case "opened":
      return {
        ...initialLedger,
        sourceRevision: action.sourceRevision,
        savedSourceRevision: action.sourceRevision,
        generatedSourceRevision: action.generatedSourceRevision,
        draftRevisions: action.versions,
        savedDraftRevisions: action.versions,
      };
    case "reset":
      return initialLedger;
  }
}

/** Source, scratch settings and each server draft have independent save baselines. */
export function useClassRevisions(unsavedMessage: string) {
  const [ledger, dispatch] = useReducer(revisionReducer, initialLedger);
  const isDirty = ledger.sourceRevision !== ledger.savedSourceRevision ||
    ledger.scratchRevision !== ledger.savedScratchRevision ||
    Object.entries(ledger.draftRevisions).some(
      ([id, revision]) => ledger.savedDraftRevisions[id] !== revision,
    );

  useEffect(() => {
    function warnBeforeUnload(event: BeforeUnloadEvent) {
      if (!isDirty) return;
      event.preventDefault();
      event.returnValue = unsavedMessage;
    }
    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [isDirty, unsavedMessage]);

  return {
    sourceRevision: ledger.sourceRevision,
    generatedSourceRevision: ledger.generatedSourceRevision,
    draftRevisions: ledger.draftRevisions,
    scratchRevision: ledger.scratchRevision,
    isDirty,
    markSourceChanged: () => dispatch({ type: "source_changed" }),
    markScratchChanged: () => dispatch({ type: "scratch_changed" }),
    clearScratchChanges: () => dispatch({ type: "scratch_cleared" }),
    rememberDraft: (id: string, revision: number) =>
      dispatch({ type: "remember_draft", id, revision }),
    forgetDrafts: (ids: string[]) => dispatch({ type: "forget_drafts", ids }),
    replaceDrafts: (versions: Record<string, number>) =>
      dispatch({ type: "replace_drafts", versions }),
    setGeneratedSourceRevision: (revision: number | null) =>
      dispatch({ type: "generated_source", revision }),
    markSaved: (sourceRevision: number, drafts: DraftVersion[], scratchRevision: number) =>
      dispatch({ type: "saved", sourceRevision, drafts, scratchRevision }),
    openBaseline: (sourceRevision: number, generatedSourceRevision: number | null, versions: Record<string, number>) =>
      dispatch({ type: "opened", sourceRevision, generatedSourceRevision, versions }),
    reset: () => dispatch({ type: "reset" }),
  };
}
