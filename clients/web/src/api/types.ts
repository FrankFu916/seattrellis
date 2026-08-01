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
  version?: string;
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
    backend?: string;
  };
};

/** A validated-enough JSON snapshot kept in memory until the next solve. */
export type HistorySnapshotPayload = Record<string, unknown>;

export type SolverBackend = "auto" | "fallback" | "ortools" | "native";

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
  backend: SolverBackend;
  customRulesJson: string;
};

export type RotationSettings = {
  enabled: boolean;
  periodCount: number;
  periodLabels: string;
};

export type GenerateClassResponse = {
  class_name: string;
  recommended_candidate_id: string;
  candidates: Array<{
    candidate_id: string;
    recommended: boolean;
    total_score: number;
  }>;
  warnings: string[];
  editor: EditorState;
};

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

export type GenerateRotationPlanResponse = {
  class_name: string;
  warnings: string[];
  rotation_plan: RotationPlan;
  editor: EditorState;
  period_editors?: EditorState[];
};

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

export type EditorOperation = {
  kind: string;
  payload: Record<string, string | null | number>;
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
