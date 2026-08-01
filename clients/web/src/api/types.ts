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

export type ExportDraftRequest = {
  draft_id: string;
  format: string;
  orientation: "portrait" | "landscape";
  locale?: "zh" | "en";
  show_student_ids?: boolean;
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

export type ProjectArtifact = {
  name: string;
  path: string;
  kind: "snapshot" | "candidate_set" | "rotation_plan" | "unknown";
  modified_at: string;
  created_at: string | null;
  size_bytes: number;
  student_count: number | null;
  period_count: number | null;
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
