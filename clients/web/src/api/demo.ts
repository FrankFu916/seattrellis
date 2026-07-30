import type {
  BootstrapData,
  CatalogResponse,
  SeatAssignment,
  Student,
} from "./types";

export const demoStudents: Student[] = [
  { id: "S01", name: "林晓雨" },
  { id: "S02", name: "陈子涵" },
  { id: "S03", name: "王乐安" },
  { id: "S04", name: "赵思远" },
  { id: "S05", name: "周语桐" },
  { id: "S06", name: "吴嘉禾" },
  { id: "S07", name: "徐知夏" },
  { id: "S08", name: "孙明哲" },
  { id: "S09", name: "胡可欣" },
  { id: "S10", name: "高一诺" },
  { id: "S11", name: "何文博" },
  { id: "S12", name: "郭心悦" },
  { id: "S13", name: "马亦辰" },
  { id: "S14", name: "罗舒然" },
  { id: "S15", name: "梁景行" },
  { id: "S16", name: "宋安宁" },
  { id: "S17", name: "郑予希" },
  { id: "S18", name: "谢承宇" },
];

export const demoCatalogs: CatalogResponse = {
  roomTemplates: [
    {
      id: "compact",
      name: { "zh-CN": "紧凑教室", en: "Compact room" },
      description: {
        "zh-CN": "4 行 × 5 列，适合小班",
        en: "4 rows × 5 columns for a smaller class",
      },
      rows: 4,
      columns: 5,
    },
    {
      id: "standard",
      name: { "zh-CN": "标准教室", en: "Standard room" },
      description: {
        "zh-CN": "5 行 × 6 列，保留充足空位",
        en: "5 rows × 6 columns with room to spare",
      },
      rows: 5,
      columns: 6,
    },
    {
      id: "wide",
      name: { "zh-CN": "宽排教室", en: "Wide room" },
      description: {
        "zh-CN": "4 行 × 7 列，适合横向教室",
        en: "4 rows × 7 columns for a wide classroom",
      },
      rows: 4,
      columns: 7,
    },
  ],
  teacherGoals: [
    {
      id: "daily-rotation",
      name: { "zh-CN": "日常轮换", en: "Daily rotation" },
      description: {
        "zh-CN": "兼顾前后排机会，适合定期换座",
        en: "Share front and back row opportunities over time",
      },
    },
    {
      id: "quick-shuffle",
      name: { "zh-CN": "快速换座", en: "Quick shuffle" },
      description: {
        "zh-CN": "快速产生一份清晰、可继续调整的座位表",
        en: "Create a clear plan quickly, then fine-tune it",
      },
    },
    {
      id: "peer-support",
      name: { "zh-CN": "同伴互助", en: "Peer support" },
      description: {
        "zh-CN": "尽量安排学习特点互补的邻座",
        en: "Place complementary learners near each other",
      },
    },
  ],
  exportFormats: [
    {
      id: "print",
      name: { "zh-CN": "打印版", en: "Print sheet" },
      description: {
        "zh-CN": "适合 A4 打印和存为 PDF",
        en: "Designed for A4 printing or saving as PDF",
      },
    },
    {
      id: "projector",
      name: { "zh-CN": "投影版", en: "Projector view" },
      description: {
        "zh-CN": "大字号、少信息，适合教室展示",
        en: "Large type and fewer details for classroom display",
      },
    },
  ],
};

export function createSeatAssignments(
  rows: number,
  columns: number,
  students: Student[],
  seatedCount = students.length,
): SeatAssignment[] {
  return Array.from({ length: rows * columns }, (_, index) => {
    const row = Math.floor(index / columns);
    const column = index % columns;
    return {
      seatId: `R${row + 1}C${column + 1}`,
      row,
      column,
      student: index < seatedCount ? students[index] : undefined,
      locked: false,
    };
  });
}

export const demoBootstrap: BootstrapData = {
  health: { status: "ok" },
  catalogs: demoCatalogs,
  source: "demo",
};

