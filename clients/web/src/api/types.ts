export type LocalizedText = {
  "zh-CN": string;
  en: string;
};

export type CatalogOption = {
  id: string;
  name: LocalizedText;
  description: LocalizedText;
};

export type RoomTemplate = CatalogOption & {
  rows: number;
  columns: number;
};

export type CatalogResponse = {
  roomTemplates: RoomTemplate[];
  teacherGoals: CatalogOption[];
  exportFormats: CatalogOption[];
};

export type HealthResponse = {
  status: "ok";
  service: string;
  api_version: string;
};

export type BootstrapData = {
  health: HealthResponse;
  catalogs: CatalogResponse;
  source: "local" | "demo";
};

export type Student = {
  id: string;
  name: string;
  gender?: string | null;
  heightCm?: number | null;
  score?: number | null;
  vision?: string | number | null;
  tags?: string[];
  needs?: string[];
  notes?: string | null;
  attributes?: Record<string, unknown>;
};

export type SeatAssignment = {
  seatId: string;
  row: number;
  column: number;
  student?: Student;
  locked: boolean;
};

export type RosterFieldName =
  | "student_id"
  | "name"
  | "gender"
  | "height_cm"
  | "score"
  | "vision"
  | "tags"
  | "needs"
  | "notes";

export type RosterColumnItem = {
  index: number;
  header: string;
};

export type RosterPreviewRow = {
  row_number: number;
  cells: (string | number | boolean | null)[];
};

export type RosterMappingItem = {
  field: RosterFieldName;
  column_index: number;
};

export type RosterMappingIssueItem = {
  code: string;
  message: string;
  field: RosterFieldName | null;
  column_indices: number[];
};

export type RosterDraftResponse = {
  draft_id: string;
  source_format: "csv" | "xlsx";
  headerless: boolean;
  row_count: number;
  column_count: number;
  columns: RosterColumnItem[];
  preview_rows: RosterPreviewRow[];
  suggested_mapping: RosterMappingItem[];
  mapping_issues: RosterMappingIssueItem[];
};

export type RosterUpdateMode = "incremental" | "replace";

export type RosterUpdatePreviewRequest = {
  mapping: RosterMappingItem[];
  mode: RosterUpdateMode;
  current_students: Student[];
  current_revision: number;
  updated_fields: RosterFieldName[];
};

export type RosterFieldChangeItem = {
  field: RosterFieldName;
  before: string | number | null;
  after: string | number | null;
};

export type RosterChangeItem = {
  action: "add" | "update" | "unchanged" | "remove" | "conflict";
  match_method: string | null;
  before: Student | null;
  after: Student | null;
  field_changes: RosterFieldChangeItem[];
  incoming_index: number | null;
  existing_index: number | null;
};

export type RosterConflictItem = {
  code: string;
  message: string;
  incoming_index: number | null;
  existing_indices: number[];
};

export type RosterUpdatePreviewResponse = {
  draft_id: string;
  base_revision: number;
  mode: RosterUpdateMode;
  can_apply: boolean;
  action_counts: Record<string, number>;
  changes: RosterChangeItem[];
  conflicts: RosterConflictItem[];
  resulting_students: Array<{
    student_id?: string | null;
    name?: string | null;
    gender?: string | null;
    height_cm?: number | null;
    score?: number | null;
    vision?: string | number | null;
    tags?: string[];
    needs?: string[];
    notes?: string | null;
    attributes?: Record<string, unknown>;
  }> | null;
};

export type EditorSeatState = {
  seat_id: string;
  row: number;
  col: number;
  enabled: boolean;
  student_key: string | null;
  locked: boolean;
};

export type EditorStudentState = {
  student_key: string;
  display_name: string;
  seat_id: string | null;
  locked: boolean;
};

export type EditorState = {
  kind: "seattrellis_editor_state";
  protocol_version: string;
  draft_id: string;
  revision: number;
  candidate_id: string | null;
  undo_depth: number;
  redo_depth: number;
  students: EditorStudentState[];
  seats: EditorSeatState[];
};

export type GenerateClassRequest = {
  draft: {
    name: string;
    students: Array<{
      student_id?: string | null;
      name?: string | null;
      gender?: string | null;
      height_cm?: number | null;
      score?: number | null;
      vision?: string | number | null;
      tags?: string[];
      needs?: string[];
      notes?: string | null;
      attributes?: Record<string, unknown>;
    }>;
    room: {
      template_id?: string;
      layout?: Record<string, unknown>;
    };
    history_snapshots?: Record<string, unknown>[];
    goal: {
      goal_id: string;
      custom_rules?: Record<string, unknown>;
      hard_rules?: HardRulesPayload;
      rules_overlay?: Record<string, unknown>;
    };
  };
  options?: {
    candidate_count?: number;
    seed?: number;
    time_limit_seconds?: number;
  };
};

/** A validated-enough JSON snapshot kept in memory until the next solve. */
export type HistorySnapshotPayload = Record<string, unknown>;

export type HardRulesPayload = {
  fixed_seats?: Array<{ student: string; seat_id: string }>;
  must_be_adjacent?: Array<{ students: [string, string] }>;
  cannot_be_adjacent?: Array<{ students: [string, string] }>;
  min_distance?: Array<{
    students: [string, string];
    distance: number;
    metric?: "euclidean" | "graph";
  }>;
};

export type CommonGroupRule = {
  id: string;
  name: string;
  mode: "together" | "separate";
  students: string[];
  /** Disabled rules keep their definition but do not reach the solver. */
  enabled?: boolean;
};

export type CommonConstraintKind =
  | "avoid_adjacent"
  | "must_adjacent"
  | "fixed_seat"
  | "min_distance";

export type CommonConstraint = {
  id: string;
  kind: CommonConstraintKind;
  first: string;
  second: string;
  seatId: string;
  distance: number;
  metric: "euclidean" | "graph";
  /** Disabled rules keep their definition but do not reach the solver. */
  enabled?: boolean;
};

export type CommonPreferenceId =
  | "vision_front"
  | "height_back"
  | "fair_rotation"
  | "avoid_recent_neighbors"
  | "score_position"
  | "score_distribution"
  | "mentor_pairing";

export type RuleRelation =
  | "desk_mate"
  | "horizontal"
  | "vertical"
  | "diagonal"
  | "adjacent_any"
  | "within_distance";

// ---------------------------------------------------------------------------
// Rule-builder sentence templates (B3 / D3)
// ---------------------------------------------------------------------------

/** Bilingual copy with the Rust registry's `zh` / `en` keys. */
export type BilingualText = Record<"zh" | "en", string>;

export type SentenceSlotKind =
  | "student"
  | "students"
  | "seat"
  | "text"
  | "number"
  | "choice";

export type SentenceSlotOption = {
  value: string;
  param_value?: unknown;
  label: BilingualText;
};

export type SentenceSlot = {
  key: string;
  kind: SentenceSlotKind;
  label: BilingualText;
  placeholder?: BilingualText | null;
  /** Slash-separated path into the template entry (e.g. "students/0"). */
  param_path?: string | null;
  required: boolean;
  options?: SentenceSlotOption[] | null;
  min?: number | null;
  max?: number | null;
  step?: number | null;
  default?: unknown;
};

export type SentenceTemplate = {
  id: string;
  rule_id: string;
  category: "hard" | "soft";
  label: BilingualText;
  sentence: BilingualText;
  slots: SentenceSlot[];
  defaults: Record<string, unknown>;
};

export type RuleTemplatesResponse = {
  api_version: "1";
  templates: SentenceTemplate[];
};

export type CompiledRule = {
  api_version: "1";
  category: "hard" | "soft";
  rule_id: string;
  entry: Record<string, unknown>;
};

export type RuleCompileError = {
  code: string;
  slot: string | null;
  message: string;
};

/** One field-level finding from `POST /api/v1/rules/validate` (M6-02). */
export type RuleDiagnostic = {
  path: string;
  code: RuleDiagnosticCode;
  detail?: string;
};

/** Stable diagnostic codes mirrored from the Rust rule validator. */
export type RuleDiagnosticCode =
  | "invalid_json"
  | "root_object"
  | "unknown_field"
  | "object_required"
  | "array_required"
  | "pair_shape"
  | "fixed_seat_shape"
  | "distance_value"
  | "group_shape"
  | "group_members"
  | "group_mode"
  | "unknown_student"
  | "unknown_seat"
  | "value_type";

export type RuleValidateResponse = {
  api_version: "1";
  diagnostics: RuleDiagnostic[];
};

export type DetailedRuleSettings = {
  enabled: boolean;
  fairRotation: {
    enabled: boolean;
    weight: number;
    lookback: number;
  };
  avoidRecentNeighbors: {
    enabled: boolean;
    weight: number;
    lookback: number;
    maxRecentCount: number;
    withinDistance: number;
    relationTypes: RuleRelation[];
  };
  cooling: {
    enabled: boolean;
    weight: number;
    coolingPeriod: number;
    withinDistance: number;
    relationTypes: RuleRelation[];
  };
  scorePosition: {
    enabled: boolean;
    weight: number;
    direction: "high_front" | "high_back";
  };
  scoreDistribution: {
    enabled: boolean;
    weight: number;
    scope: "row" | "group";
  };
  mentorPairing: {
    enabled: boolean;
    weight: number;
    mentorPercentile: number;
    learnerPercentile: number;
    relation: RuleRelation;
    avoidRecentRepeats: boolean;
    historyLookback: number;
  };
};

export type CustomRoomSettings = {
  enabled: boolean;
  rows: number;
  columns: number;
  aisleColumns: string;
  disabledSeats: string;
  layoutJson: string;
};

export type LayoutCellKind = "seat" | "aisle" | "platform" | "empty";

export type LayoutCellState = {
  row: number;
  column: number;
  kind: LayoutCellKind;
  seat_id: string | null;
};

export type LayoutStateResponse = {
  kind: "seattrellis_layout_state";
  api_version: "1";
  draft_id: string;
  revision: number;
  name: string;
  rows: number;
  columns: number;
  cells: LayoutCellState[];
  undo_depth: number;
  redo_depth: number;
  usable_seat_count: number;
};

export type CreateLayoutDraftRequest = {
  name?: string;
  template_id?: string;
  layout?: Record<string, unknown>;
  rows?: number;
  columns?: number;
};

export type LayoutOperation = {
  kind:
    | "set_cell"
    | "insert_row"
    | "delete_row"
    | "insert_column"
    | "delete_column"
    | "translate"
    | "mirror_horizontal"
    | "flip_vertical";
  payload?: Record<string, string | number | null>;
};

export type LayoutCommand = {
  command_id: string;
  draft_id: string;
  base_revision: number;
  action: "apply" | "undo" | "redo";
  operation?: LayoutOperation;
};

export type CompiledLayoutResponse = {
  api_version: "1";
  draft_id: string;
  revision: number;
  layout: Record<string, unknown>;
};

export type AdvancedSolveSettings = {
  candidateCount: number;
  seed: string;
  timeLimitSeconds: number;
  customRulesJson: string;
};

export type RotationSettings = {
  enabled: boolean;
  periodCount: number;
  periodLabels: string;
};

export type SolveStatus =
  | "Solved"
  | "ProvenInfeasible"
  | "Timeout"
  | "Unknown"
  | "InvalidInput"
  | "Cancelled"
  | "InternalError";

export type NormalUnsolvedStatus = Exclude<
  SolveStatus,
  "Solved" | "InvalidInput" | "InternalError"
>;

export type CandidateSummary = {
  candidate_id: string;
  recommended: boolean;
  total_score: number;
};

/** The `goal` object the server echoes in every generate response. */
export type GenerateClassGoal = {
  goal_id: string;
  title: string;
  description: string;
  preset_name: string | null;
};

export type GenerateClassSolvedResponse = {
  status: "Solved";
  feasible: true;
  class_name: string;
  goal: GenerateClassGoal;
  recommended_candidate_id: string;
  candidates: CandidateSummary[];
  warnings: string[];
  editor: EditorState;
};

export type GenerateClassUnsolvedResponse = {
  status: NormalUnsolvedStatus;
  feasible: false;
  class_name: string;
  goal: GenerateClassGoal;
  recommended_candidate_id: null;
  candidates: [];
  warnings: string[];
  editor: null;
  message_key: string;
  recoverable: boolean;
  suggested_action: string;
};

export type GenerateClassResponse =
  | GenerateClassSolvedResponse
  | GenerateClassUnsolvedResponse;

export type RotationPeriod = {
  period: number;
  label: string;
  snapshot: {
    assignments: Array<{
      student_key: string;
      student_name: string;
      seat_id: string;
    }>;
    solver_status: string;
  };
};

export type RotationPlan = {
  schema_version?: string;
  kind: "rotation_plan";
  created_at?: string;
  name: string;
  periods: RotationPeriod[];
  base_history_count: number;
  fairness_summary: Record<string, unknown>;
  pair_repeat_summary: Record<string, unknown>;
  warnings: string[];
  metadata?: Record<string, unknown>;
};

export type GenerateRotationPlanRequest = {
  draft: GenerateClassRequest["draft"];
  period_count: number;
  period_labels?: string[];
  options?: GenerateClassRequest["options"];
};

export type GenerateRotationPlanSolvedResponse = {
  status: "Solved";
  feasible: true;
  class_name: string;
  warnings: string[];
  rotation_plan: RotationPlan;
  editor: EditorState;
  failed_period: null;
  period_editors?: EditorState[];
};

export type GenerateRotationPlanUnsolvedResponse = {
  status: NormalUnsolvedStatus;
  feasible: false;
  class_name: string;
  warnings: string[];
  rotation_plan: null;
  editor: null;
  failed_period: number | null;
  message_key: string;
  recoverable: boolean;
  suggested_action: string;
};

export type GenerateRotationPlanResponse =
  | GenerateRotationPlanSolvedResponse
  | GenerateRotationPlanUnsolvedResponse;

export type ExportDraftRequest = {
  draft_id: string;
  format: string;
  template: "public" | "teacher" | "report";
  privacy: ExportPrivacyOptions;
  orientation: "portrait" | "landscape";
  page_scale: number;
  locale?: "zh" | "en";
  show_student_ids?: boolean;
};

export type ExportTemplate = "public" | "teacher" | "report";

export type ExportPrivacyOptions = {
  hide_scores: boolean;
  hide_notes: boolean;
  hide_special_needs: boolean;
  anonymize: boolean;
  show_height: boolean;
  show_vision: boolean;
};

/** Scalar payload of a simple editor operation. */
export type EditorOperationPayload = Record<
  string,
  string | null | number
>;

/** Payload of the atomic `batch_move` operation (Rust editing protocol). */
export type BatchMovePayload = {
  moves: Array<{ student_key: string; seat_id: string }>;
};

export type EditorOperation = {
  kind: string;
  payload: EditorOperationPayload | BatchMovePayload;
};

export type EditorCommand = {
  kind: "seattrellis_editor_command";
  protocol_version: string;
  command_id: string;
  draft_id: string;
  base_revision: number;
  action: "apply" | "undo" | "redo";
  operations: EditorOperation[];
};

export type RecentProject = {
  name: string;
  path: string;
  modified_at: string;
};

export type ProjectListResponse = {
  api_version: "1";
  root: string;
  projects: RecentProject[];
};

export type ProjectArtifactProvenance = {
  source: "generated" | "manual_edit" | "rotation_edit" | "restored" | "unknown";
  parent_name: string | null;
  operation_count: number | null;
};

export type ProjectArtifactOperation = {
  sequence: number;
  action: "apply" | "undo" | "redo" | "unknown";
  operation_count: number;
  operation_kinds: string[];
  period?: number | null;
  recorded_at?: string | null;
};

export type ProjectArtifact = {
  name: string;
  path: string;
  kind: "snapshot" | "candidate_set" | "rotation_plan" | "unknown";
  modified_at: string;
  created_at: string | null;
  size_bytes: number;
  student_count: number | null;
  period_count: number | null;
  provenance: ProjectArtifactProvenance | null;
  operation_history?: ProjectArtifactOperation[];
  operation_history_truncated?: boolean;
};

export type ProjectHistoryResponse = {
  api_version: "1";
  project_name: string;
  project_path: string;
  history: ProjectArtifact[];
  outputs: ProjectArtifact[];
  warnings: string[];
};

export type ProjectArtifactSummary = {
  name: string;
  path: string;
  kind: "snapshot" | "candidate_set" | "rotation_plan" | "unknown";
  created_at: string | null;
  student_count: number | null;
  assignment_count: number | null;
  enabled_seat_count: number | null;
  solver_status: string | null;
};

export type ProjectArtifactDiff = {
  assignment_changes: number;
  roster_added: number;
  roster_removed: number;
  layout_changed: boolean;
  rules_changed: boolean;
  solver_status_changed: boolean;
  assignment_details: Array<{
    student_ref: string;
    change: "moved" | "seated" | "unseated";
    before_seat_id: string | null;
    after_seat_id: string | null;
  }>;
};

export type ProjectArtifactCompareResponse = {
  api_version: "1";
  left: ProjectArtifactSummary;
  right: ProjectArtifactSummary;
  diff: ProjectArtifactDiff;
};

export type ProjectPrivacyFinding = {
  file: string;
  fields: string[];
};

export type ProjectPrivacyResponse = {
  api_version: "1";
  project_path: string;
  files_scanned: number;
  safe_for_public_sharing: boolean;
  findings: ProjectPrivacyFinding[];
};

export type ProjectRestoreResponse = {
  api_version: "1";
  project_path: string;
  output_dir: string;
};

export type ProjectArtifactRestoreResponse = {
  api_version: "1";
  project_path: string;
  source_artifact: string;
  restored_artifact: string;
};

export type ProjectGroupMemberChange = {
  student_ref: string;
  change: "added" | "removed";
};

export type ProjectGroupPreview = {
  name: string;
  member_count: number;
  seated_count: number;
  unseated_count: number;
  missing_count: number;
  added_count: number;
  removed_count: number;
  member_changes: ProjectGroupMemberChange[];
};

export type ProjectGroupRegisterPeriodPreview = {
  period: number;
  label: string;
  compared_to_period: number | null;
  groups: ProjectGroupPreview[];
};

export type ProjectGroupRegisterPreviewResponse = {
  api_version: "1";
  project_path: string;
  artifact_path: string;
  plan_name: string;
  period_count: number;
  periods: ProjectGroupRegisterPeriodPreview[];
  has_changes: boolean;
};

export type ProjectMigrationResponse = {
  api_version: "1";
  project_path: string;
  source_path: string;
  artifact: string;
  schema_version: string | number;
  output_path: string | null;
  backup_path: string | null;
  dry_run: boolean;
  before_valid: boolean;
  after_valid: boolean | null;
  rollback_available: boolean;
  change_count: number;
  changes: ProjectMigrationChange[];
  reference_checks?: ProjectMigrationReferenceCheck[];
};

export type ProjectMigrationSharedReference = {
  path: string;
  projects: string[];
  fields: string[];
};

export type ProjectMigrationBatchResponse = {
  api_version: "1";
  projects: ProjectMigrationResponse[];
  shared_references: ProjectMigrationSharedReference[];
  ready: boolean;
};

export type ProjectMigrationReferenceCheck = {
  field: "students" | "layout" | "rules" | "history_dir" | "outputs_dir";
  path: string;
  expected: "file" | "directory";
  status: "ok" | "missing" | "wrong_type";
};

export type ProjectMigrationRestoreResponse = {
  api_version: "1";
  project_path: string;
  source_path: string;
  backup_path: string;
  safety_backup_path: string | null;
  artifact: string;
  schema_version: string | number;
  restored_valid: boolean;
};

export type ProjectMigrationChange = {
  path: string;
  change: "added" | "removed" | "changed";
  before_type: string | null;
  after_type: string | null;
};

export type ProjectRotationSaveResponse = {
  api_version: "1";
  project_path: string;
  output_path: string;
  period_count: number;
  saved_at: string;
};

export type ProjectRotationLoadResponse = {
  api_version: "1";
  project_path: string;
  artifact_path: string;
  rotation_plan: RotationPlan;
  editor: EditorState;
  period_editors: EditorState[];
};

// ---------------------------------------------------------------------------
// Draft audit report (B5 / D5, D6)
// ---------------------------------------------------------------------------

export type AuditDimension = {
  status: "available" | "not_available";
  score?: number | null;
  weight?: number | null;
  details?: Record<string, unknown>;
};

export type HardConstraintSummary = {
  all_satisfied: boolean;
  checked_rule_count: number;
  violation_count: number;
  witnesses: unknown[];
};

export type SuggestedAction = {
  message_key: string;
  suggested_action: string;
  args: Record<string, string | number>;
};

export type DraftAuditReport = {
  api_version: string;
  draft_id: string;
  feasible: boolean;
  score: {
    total: number;
    breakdown: {
      fair_rotation_score: AuditDimension;
      avoid_recent_neighbors_score: AuditDimension;
      score_balance_score: AuditDimension;
      height_preference_score: AuditDimension;
      vision_preference_score: AuditDimension;
      diversity_score: AuditDimension;
      stability_score: AuditDimension;
      rule_scores: Record<string, AuditDimension>;
      hard_constraint_summary: HardConstraintSummary;
    };
  };
  audit: {
    hard_constraint_summary: HardConstraintSummary;
    missing_data: Record<string, number>;
    history: { snapshot_count: number; has_history: boolean };
    suggested_actions: SuggestedAction[];
    [key: string]: unknown;
  };
};
