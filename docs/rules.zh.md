# 排座规则手册

[English](rules.md) · [简体中文](rules.zh.md)

在 **席序（SeatTrellis）** 中，排座规则被严格划分为两类：
- **硬约束（Hard Constraints）**：必须无条件满足的底线规则。若无法全部满足，方案判定为不可行并提示冲突原因。
- **软偏好（Soft Preferences）**：加权优化的教学目标。算法将在满足所有硬约束的前提下，尽量追求总分最高。

---

## 🧩 1. 硬约束（Hard Constraints）

硬约束用于定义班级座位的刚性边界。求解器在计算过程中将硬约束作为剪枝与可行性验证的核心依据。

```json
{
  "seed": 42,
  "hard": {
    "fixed_seats": [
      { "student": "STU001", "seat_id": "R1C1" }
    ],
    "must_be_adjacent": [
      { "students": ["STU002", "STU003"] }
    ],
    "cannot_be_adjacent": [
      { "students": ["STU004", "STU005"] }
    ],
    "min_distance": [
      { "students": ["STU006", "STU007"], "distance": 2, "metric": "euclidean" }
    ]
  }
}
```

### 硬约束类型一览表

| 规则字段 | 说明 | 适用场景 |
| :--- | :--- | :--- |
| `fixed_seats` | **固定座位**：将指定学生绑定到特定可用座位。 | 班干部固定坐讲台旁、特殊需求学生固定位。 |
| `must_be_adjacent` | **必须相邻**：要求两名学生必须为相邻座位（横向/纵向/连通邻近）。 | 结对帮扶、实验搭档、互助小组。 |
| `cannot_be_adjacent` | **禁止相邻**：禁止两名学生处于相邻位置。 | 避免课堂交头接耳、化解学生矛盾。 |
| `min_distance` | **最小物理距离**：要求两人之间的欧几里得/切比雪夫距离不小于指定阈值。 | 考试防窥隔离、重点关注学生分散布局。 |

> 📌 **引用规范**：
> - `student` 可填入学生学号（`student_id`）或姓名（`name`）；
> - `seat_id` 必须在当前教室布局中真实存在，且状态必须为启用（`enabled: true`）。

### 自动化冲突预检机制

运行预检命令可在求解前快速发现逻辑矛盾：

```bash
seattrellis validate --problem problem.json
```

系统将自动拦截并报告以下冲突：
1. 引用了不存在的学生或已禁用的座位；
2. 同一名学生被固定到多个不同座位；
3. 同一个座位被分配给了多名学生；
4. 同一对学生同时出现在 `must_be_adjacent` 与 `cannot_be_adjacent` 中；
5. `min_distance` 与 `must_be_adjacent` 产生直接冲突；
6. 固定的座位本身已违反相邻或最小间距要求。

---

## 🎯 2. 软偏好（Soft Preferences）

软偏好用于引导求解器向更理想的教学目标优化。每项软规则均包含是否启用（`enabled`）以及优化权重（`weight`，范围为 `0` ~ `1,000,000`）。

```json
{
  "soft": {
    "vision_front": { "enabled": true, "weight": 20 },
    "height_back": { "enabled": true, "weight": 5 },
    "randomize": { "enabled": true, "weight": 1 },
    "score_balance": { "enabled": false, "weight": 10 },
    "fair_rotation": {
      "enabled": true,
      "weight": 15,
      "avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"],
      "lookback": 4
    },
    "avoid_recent_neighbors": {
      "enabled": true,
      "weight": 10,
      "lookback": 4,
      "relation_types": ["desk_mate", "adjacent_any"],
      "max_recent_count": 1,
      "within_distance": 2
    }
  }
}
```

### 常用软规则说明

| 规则标识 | 核心作用 | 依赖数据 |
| :--- | :--- | :--- |
| `vision_front` | **视力优先靠前**：视力较弱或标记有靠前需求的学生尽量安排在前排。 | 学生属性 `needs_front` / `vision_score` |
| `height_back` | **高个靠后排列**：按身高梯度排布，个子高的同学尽量安排在中后排，避免遮挡视线。 | 学生属性 `height` |
| `randomize` | **可复现随机微调**：基于 Seed 注入受控扰动，在满足目标的同时保持排座自然。 | 随机种子 `seed` |
| `score_balance` | **成绩均衡分布**：控制相邻或同组学生的学业成绩层次，实现“以优带潜”或分层混合。 | 学生属性 `score` |
| `fair_rotation` | **历史位置公平轮换**：分析历史排座，避免学生连续多期坐在前排、后排、靠门窗或角落。 | 历史快照文件（`*.snapshot.json`） |
| `avoid_recent_neighbors` | **近期同桌回避**：统计近期同桌与相邻关系，避免同一对学生长期重复搭档。 | 历史快照文件 |
| `cooling` | **严格关系冷却期**：在设定的轮换期数（`cooling_period`）内严格避免再次同桌。 | 历史快照文件 |

---

## 🔄 3. 历史轮换与公平性机制

### 3.1 `fair_rotation`（位置类别轮换）

长期班级排座最忌讳“个别学生一直坐角落”或“某同学始终在空调风口”。`fair_rotation` 会为每个学生建立历史位置画像：

- **支持追踪的位置类别**：
  - `front`（前排）
  - `back`（后排）
  - `side`（两侧靠墙）
  - `corner`（四角边缘）
  - `near_window`（靠窗）
  - `near_door`（靠门）
  - `near_ac`（靠空调风口）
  - `near_platform`（靠讲台）

> 💡 **参数详解**：
> - `lookback`: 近期回溯期数（例如设为 4，表示重点考察最近 4 次排座记录）；
> - 若未提供历史数据，该规则将自动优雅降级，评分标记为 `not_available`，不会中断求解。

---

### 3.2 `avoid_recent_neighbors` 与 `cooling`（同桌搭档回避）

为了增进同学交流或避免小团体产生，系统支持多粒度的搭档回避策略：

| 关系类型 (`relation_types`) | 判定标准 |
| :--- | :--- |
| `desk_mate` | **标准同桌**：同排且列差为 1 的水平紧邻座位。 |
| `horizontal` | **横向相邻**：同一排、相邻列。 |
| `vertical` | **纵向前后**：同一列、前后排。 |
| `diagonal` | **对角斜向**：斜对角相邻。 |
| `adjacent_any` | **任意相邻**：涵盖横向、纵向、斜向及自定义连通边缘。 |
| `within_distance` | **范围邻近**：切比雪夫几何距离满足 `distance <= within_distance`。 |

---

## 📋 4. 开箱即用的 14 种场景预设（Presets）

在实际使用中，您不必每次手动编写冗长的规则。SeatTrellis 内置了 14 种标准预设：

```bash
seattrellis validate --problem problem.json --preset daily --history-dir examples/history
```

| 预设名称 | 适用场景 | 规则侧重 |
| :--- | :--- | :--- |
| `daily` | **日常教学综合** | 均衡兼顾视力靠前、身高靠后、成绩混合、公平轮换与同桌回避。 |
| `exam` | **考场标准化** | 强随机扰动，配合硬约束中的最小间距进行防作弊排座。 |
| `random` | **快速随机打乱** | 仅启用可复现的纯随机扰动，适合快速活动。 |
| `fair-rotation` | **周期轮换优先** | 重点消除位置偏好偏差，让每位同学均等体验各个区域。 |
| `neighbor-aware` | **搭档频繁更新** | 最大限度打乱社交圈，让全班同学建立更广泛的联系。 |
| `balanced` | **成绩互助分层** | 优先实现相邻座位的成绩梯度互补。 |
| `height-aware` | **严格身高排布** | 适合高低个头差异悬殊的班级，彻底杜绝视线遮挡。 |
| `vision-friendly` | **视力关爱优先** | 重点保障近视或散光同学的前排与居中视角。 |

---

## 📊 5. 多维度评分体系与报告解读

系统对每个方案在 `0` ~ `100` 分区间进行标准化打分：

| 评分维度 | 评估含义 | 得分判定 |
| :--- | :--- | :--- |
| `vision_preference_score` | 视力需求学生的靠前度 | 视力困难学生越靠前，分数越高。 |
| `height_preference_score` | 身高梯度的吻合度 | 高个靠后、矮个靠前匹配越好，分数越高。 |
| `fair_rotation_score` | 历史位置轮换的公平性 | 避免了历史重复区域，轮换越充分分数越高。 |
| `avoid_recent_neighbors_score` | 同桌搭档的新鲜度 | 避开了近期同桌组合，分数越高。 |
| `score_balance_score` | 成绩互助的均衡度 | 达到预期成绩搭配梯度，分数越高。 |
| `diversity_score` | 相比其他候选方案的差异度 | 多套方案对比时，座位变化越丰富得分越高。 |
| `stability_score` | 相比上一期座位的稳定性 | 需保持部分人员不动时，留座率越高得分越高。 |

> ⚠️ **关于 `not_available` 的说明**：
> 当缺少必要数据（例如未导入历史记录或学生未标注成绩）时，该维度将显示为 `not_available`，系统**不会虚构 0 分**，也不会将该维度计入加权总分分母，确保评估真实诚实。
