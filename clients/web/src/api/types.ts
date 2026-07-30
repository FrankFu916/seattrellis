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

export type RosterUpdateMode = "incremental" | "overwrite";

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
  resulting_students: Student[] | null;
};

