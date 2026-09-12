import { deleteEditorDraft, fetchEditorState } from "../api/client";
import type {
  CandidateSummary,
  EditorState,
  GenerateClassSolvedResponse,
  GenerateRotationPlanSolvedResponse,
  RotationPlan,
} from "../api/types";

type GeneratedResponse =
  GenerateClassSolvedResponse | GenerateRotationPlanSolvedResponse;
export type PreparedGeneration = {
  editor: EditorState;
  periodEditors: EditorState[];
  rotationPlan: RotationPlan | null;
  candidates: Array<{ summary: CandidateSummary; editor: EditorState }>;
};

/** Acquire all drafts before publishing a generation. Unclaimed drafts are
 * released even when the user changes class or a required editor fails to load.
 * commit must be synchronous so the liveness check and state update are atomic.
 */
export async function consumeGeneratedDrafts(
  response: GeneratedResponse,
  isCurrent: () => boolean,
  commit: (generation: PreparedGeneration) => void,
): Promise<void> {
  const periodEditors =
    "rotation_plan" in response
      ? response.period_editors?.length
        ? response.period_editors
        : [response.editor]
      : [];
  const summaries = "candidates" in response ? response.candidates : [];
  const primaryId = periodEditors[0]?.draft_id ?? response.editor.draft_id;
  const owned = new Set([
    response.editor.draft_id,
    ...periodEditors.map((editor) => editor.draft_id),
    ...summaries.map((candidate) => candidate.candidate_id),
  ]);
  const retained = new Set<string>();
  try {
    if (!isCurrent()) return;
    const wanted = [
      ...new Set([
        primaryId,
        ...summaries.map((candidate) => candidate.candidate_id),
      ]),
    ];
    const results = await Promise.allSettled(
      wanted.map((id) => fetchEditorState(id)),
    );
    if (!isCurrent()) return;
    const primary = results[0];
    if (primary.status === "rejected") throw primary.reason;
    const editors = new Map<string, EditorState>();
    results.forEach((result, index) => {
      if (result.status === "fulfilled")
        editors.set(wanted[index], result.value);
    });
    const candidates = summaries.flatMap((summary) => {
      const editor = editors.get(summary.candidate_id);
      return editor ? [{ summary, editor }] : [];
    });
    commit({
      editor: primary.value,
      periodEditors,
      rotationPlan: "rotation_plan" in response ? response.rotation_plan : null,
      candidates,
    });
    retained.add(primaryId);
    periodEditors.forEach((editor) => retained.add(editor.draft_id));
    candidates.forEach(({ editor }) => retained.add(editor.draft_id));
  } finally {
    await Promise.allSettled(
      [...owned]
        .filter((id) => !retained.has(id))
        .map((id) => deleteEditorDraft(id)),
    );
  }
}
