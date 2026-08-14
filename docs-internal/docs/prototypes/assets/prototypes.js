/* ---------------------------------------------------------------------------
 * SeatTrellis v2 M4 prototype gallery — shared behaviour.
 *
 * Two decision modes:
 *   - "confirm" (batch 1, D1-D8): merged target form, footer offers
 *     "confirm" / "suggest changes";
 *   - "choose"  (batch 2, D9-D15): competing options, footer offers
 *     A/B/C/D + "redesign".
 * Choices persist in localStorage under `seattrellis-prototypes:v1`.
 * ------------------------------------------------------------------------- */

window.PTUI = (function () {
  "use strict";

  var STORAGE_KEY = "seattrellis-prototypes:v1";

  var DECISIONS = [
    // ---- batch 1: core workflow (decided, merged target forms) ----
    { id: "d1", n: 1, title: "班级主导航", file: "decisions/d1-navigation.html", mode: "confirm",
      merged: "侧栏骨架 + 上下文操作条 + 临时工作台" },
    { id: "d2", n: 2, title: "Seating canvas 交互", file: "decisions/d2-canvas.html", mode: "confirm",
      merged: "画布主视图（拖拽整块跟随 + 框选）+ 表格视图并存" },
    { id: "d3", n: 3, title: "Rule builder", file: "decisions/d3-rule-builder.html", mode: "confirm",
      merged: "句式构建器（创建）+ 规则卡片（管理）+ 高级表单兜底" },
    { id: "d4", n: 4, title: "快速 vs 高级模式", file: "decisions/d4-quick-advanced.html", mode: "confirm",
      merged: "默认精简 + 历史范围可见 + 高级折叠区" },
    { id: "d5", n: 5, title: "候选方案比较", file: "decisions/d5-candidates.html", mode: "confirm",
      merged: "通俗推荐理由 + 差异高亮 + 明细切换（去术语化）" },
    { id: "d6", n: 6, title: "可行性诊断", file: "decisions/d6-diagnostics.html", mode: "confirm",
      merged: "内联徽章 + 问题列表双向联动 + 一键修复" },
    { id: "d7", n: 7, title: "历史 / 轮换模型", file: "decisions/d7-history-rotation.html", mode: "confirm",
      merged: "历史回顾（时间线）+ 轮换计划 双视图" },
    { id: "d8", n: 8, title: "导入确认流程", file: "decisions/d8-import.html", mode: "confirm",
      merged: "步骤条 + 映射/预览同屏（班级上下文内面板）" },
    // ---- batch 2: export & wrap-up (decided, merged target forms) ----
    { id: "d9", n: 9, title: "导出面板分层", file: "decisions/d9-export.html", mode: "confirm",
      merged: "快速导出菜单 + 默认值矩阵（默认值/高级设置分层）" },
    { id: "d10", n: 10, title: "新手引导", file: "decisions/d10-onboarding.html", mode: "confirm",
      merged: "内嵌任务引导 + 内嵌示例名单兜底（示例数据隔离）" },
    { id: "d11", n: 11, title: "print-html 版式", file: "decisions/d11-print-html.html", mode: "confirm",
      merged: "独立 print 版式 + 打印版式设计规范" },
    { id: "d12", n: 12, title: "PDF CJK 字体", file: "decisions/d12-pdf-cjk.html", mode: "confirm",
      merged: "系统字体智能引用（质量优先级 + 导出警告，无嵌入无 fallback）" },
    { id: "d13", n: 13, title: "PNG 文字渲染", file: "decisions/d13-png-text.html", mode: "confirm",
      merged: "PNG 渲染学生姓名（复用字体发现 + 隐私生效）" },
    { id: "d14", n: 14, title: "原生文件对话框", file: "decisions/d14-native-dialogs.html", mode: "confirm",
      merged: "拖拽 + 系统对话框 + 可信根路径输入（三入口融合）" },
    { id: "d15", n: 15, title: "遗留命令去留", file: "decisions/d15-legacy-commands.html", mode: "confirm",
      merged: "init-demo / presets / workspace / desktop 全部移除（REMOVED_V2）" }
  ];

  var REDESIGN = { key: "R", label: "都不满意，重新设计" };
  var CONFIRMED = "confirmed";
  var REVISE = "revise";

  /* ---------- persistence ---------- */

  function load() {
    try { return JSON.parse(localStorage.getItem(STORAGE_KEY)) || {}; } catch (e) { return {}; }
  }

  function save(state) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  }

  function getChoice(id) {
    return load()[id] || null;
  }

  function setChoice(id, choice) {
    var state = load();
    state[id] = choice;
    save(state);
  }

  function decisionIndex(id) {
    for (var i = 0; i < DECISIONS.length; i++) {
      if (DECISIONS[i].id === id) return i;
    }
    return -1;
  }

  function baseDir() {
    return /\/decisions\//.test(location.pathname) ? "../" : "";
  }

  function decisionInfo(id) {
    var d = DECISIONS[decisionIndex(id)];
    var idx = decisionIndex(id);
    var prev = idx > 0 ? DECISIONS[idx - 1] : null;
    var next = idx < DECISIONS.length - 1 ? DECISIONS[idx + 1] : null;
    return { d: d, idx: idx, prev: prev, next: next };
  }

  /* ---------- footer bar ---------- */

  function renderBar(id, labels) {
    var bar = document.getElementById("decisionBar");
    if (!bar) return;
    var info = decisionInfo(id);
    var d = info.d;
    var chosen = getChoice(id);
    var html = "";

    if (d.mode === "choose") {
      html += '<span class="db-label">决策 <b>D' + d.n + '</b> · ' + d.title + ' — 选择你偏好的方案：</span>';
      labels.forEach(function (label, i) {
        var key = String.fromCharCode(65 + i);
        html += '<button class="db-btn' + (chosen === key ? " picked" : "") + '" data-choice="' + key + '">' +
          '<span class="key">' + key + "</span>" + label + "</button>";
      });
      html += '<button class="db-btn redesign' + (chosen === REDESIGN.key ? " picked" : "") + '" data-choice="' + REDESIGN.key + '">' +
        '<span class="key">R</span>' + REDESIGN.label + "</button>";
    } else {
      html += '<span class="db-label">决策 <b>D' + d.n + '</b> · ' + d.title +
        ' — 融合方案：<b style="color:#93c5fd">' + d.merged + "</b></span>";
      html += '<button class="db-btn confirm' + (chosen === CONFIRMED ? " picked" : "") +
        '" data-choice="' + CONFIRMED + '"><span class="key">✓</span>确认此方案</button>';
      html += '<button class="db-btn' + (chosen === REVISE ? " picked redesign" : "") +
        '" data-choice="' + REVISE + '"><span class="key">✎</span>有修改建议</button>';
    }

    html += '<span class="db-sep"></span>';
    html += '<a class="db-nav" href="' + baseDir() + 'index.html">← 决策总览</a>';
    if (info.prev) html += '<a class="db-nav" href="' + info.prev.file + '">← D' + info.prev.n + "</a>";
    if (info.next) html += '<a class="db-nav" href="' + info.next.file + '">D' + info.next.n + " →</a>";
    bar.innerHTML = html;

    bar.querySelectorAll(".db-btn").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var choice = btn.getAttribute("data-choice");
        setChoice(id, choice);
        renderBar(id, labels);
        if (d.mode === "choose") {
          var msg = choice === REDESIGN.key
            ? "已记录：对 D" + d.n + " 不满意，希望重新设计"
            : "已选择方案 " + choice + "（" + labels[choice.charCodeAt(0) - 65] + "）";
          toast(msg);
          document.querySelectorAll(".variant").forEach(function (card, i) {
            card.classList.toggle("v-chosen", String.fromCharCode(65 + i) === choice);
          });
        } else {
          toast(choice === CONFIRMED
            ? "已确认 D" + d.n + " 融合方案：" + d.merged
            : "已记录修改建议（D" + d.n + "）——融合方案将据此迭代");
          var main = document.querySelector(".variant");
          if (main) main.classList.toggle("v-chosen", choice === CONFIRMED);
        }
      });
    });
  }

  /* ---------- index page ---------- */

  function renderIndex() {
    var state = load();
    var done = 0;
    var grid = document.getElementById("idxGrid");
    var lastBatch = "";

    DECISIONS.forEach(function (d) {
      var batch = d.n <= 8 ? "batch1" : "batch2";
      if (batch !== lastBatch) {
        var sep = document.createElement("div");
        sep.className = "idx-sep";
        sep.innerHTML = '<span class="chip ' + (batch === "batch1" ? "green" : "blue") + '">' +
          (batch === "batch1" ? "批 1 · 核心工作流（已决策，确认融合形态）" : "批 2 · 导出与收尾（待决策）") + "</span>";
        grid.appendChild(sep);
        lastBatch = batch;
      }

      var choice = state[d.id];
      if (choice) done++;
      var card = document.createElement("a");
      card.className = "idx-card";
      card.href = d.file;
      var stateHtml;
      if (d.mode === "choose") {
        stateHtml = choice
          ? '<span class="pick">已选 ' + choice + " — " + (choice === REDESIGN.key ? REDESIGN.label : d.variants[choice.charCodeAt(0) - 65]) + "</span>"
          : '<span class="none">尚未选择</span>';
      } else {
        if (choice === CONFIRMED) stateHtml = '<span class="pick">✓ 已确认</span>';
        else if (choice === REVISE) stateHtml = '<span class="pick" style="color:var(--warn)">✎ 有修改建议</span>';
        else stateHtml = '<span class="none">待确认</span>';
      }
      card.innerHTML =
        '<div class="ic-top"><span class="v-badge">D' + d.n + "</span>" +
        '<span class="ic-title">' + d.title + "</span></div>" +
        '<div class="ic-sub">' + (d.mode === "choose" ? d.variants.join(" / ") : "融合方案：" + d.merged) + "</div>" +
        '<div class="ic-state">' + stateHtml + "</div>";
      grid.appendChild(card);
    });

    var pct = Math.round((done / DECISIONS.length) * 100);
    document.getElementById("idxFill").style.width = pct + "%";
    document.getElementById("idxPct").textContent = pct + "%";
    document.getElementById("idxDone").textContent = done + " / " + DECISIONS.length + " 项已决策";
    document.getElementById("idxHint").textContent =
      "批 1（D1–D8）决策记录：docs/product-decisions/2026-08-10-batch1-core-workflow.md。批 2（D9–D15）逐页选择 A/B/C/D，选完生成批 2 决策记录。";
    renderSummary(state);
  }

  function renderSummary(state) {
    var lines = ["# SeatTrellis v2 M4 — Product Decision Summary", "", "## Batch 1（已冻结）", ""];
    DECISIONS.filter(function (d) { return d.n <= 8; }).forEach(function (d) {
      var choice = state[d.id];
      var line = choice === CONFIRMED ? "- D" + d.n + " " + d.title + ": **confirmed** — " + d.merged
        : choice === REVISE ? "- D" + d.n + " " + d.title + ": **changes requested** — " + d.merged
        : "- D" + d.n + " " + d.title + ": _pending_ — " + d.merged;
      lines.push(line);
    });
    lines.push("", "## Batch 2（方向已冻结，细节待研究）", "");
    DECISIONS.filter(function (d) { return d.n > 8; }).forEach(function (d) {
      var choice = state[d.id];
      var line = choice === CONFIRMED ? "- D" + d.n + " " + d.title + ": **confirmed** — " + d.merged
        : choice === REVISE ? "- D" + d.n + " " + d.title + ": **changes requested** — " + d.merged
        : "- D" + d.n + " " + d.title + ": _pending_ — " + d.merged;
      lines.push(line);
    });
    lines.push("", "Batch-1 record: docs/product-decisions/2026-08-10-batch1-core-workflow.md");
    lines.push("Batch-2 record: docs/product-decisions/2026-08-10-batch2-export-wrapup.md");
    document.getElementById("idxSummaryText").value = lines.join("\n");
  }

  /* ---------- shared seat-grid renderer ---------- */

  function renderSeatGrid(container, opts) {
    var plan = opts.plan || null;
    var locks = opts.locks || [];
    var violations = opts.violations || [];
    var selected = opts.selected || [];
    var dimmed = opts.dimmed || [];
    var tags = opts.tags || {};

    var html = '<div class="seat-grid" style="grid-template-columns:repeat(' + PT.cols + ", 74px)\">";
    html += '<div class="platform">讲台</div>';
    PT.seats.forEach(function (seat) {
      var cls = ["seat"];
      if (locks.indexOf(seat.id) !== -1) cls.push("locked");
      if (violations.indexOf(seat.id) !== -1) cls.push("violation");
      if (selected.indexOf(seat.id) !== -1) cls.push("selected");
      if (dimmed.indexOf(seat.id) !== -1) cls.push("dim");
      var student = plan ? plan.assignment[seat.id] : null;
      var st = student ? PT.byId[student] : null;
      var name = st ? st.name : (seat.enabled === false ? "禁用" : "空座");
      var idText = st ? st.id : seat.id;
      var tag = tags[seat.id] ? '<span class="s-tag">' + tags[seat.id] + "</span>" : "";
      if (st && st.needs && st.needs.indexOf("vision_front") !== -1 && !tags[seat.id]) {
        tag = '<span class="s-tag">视</span>';
      }
      html += '<div class="' + cls.join(" ") + '" title="' + seat.id + " · " + seat.zone + '">' +
        tag + '<span class="s-name">' + name + "</span>" +
        '<span class="s-id">' + idText + "</span></div>";
    });
    html += "</div>";
    container.innerHTML = html;
  }

  /* ---------- helpers ---------- */

  function toast(msg) {
    var el = document.getElementById("toast") ||
      (function () {
        var t = document.createElement("div");
        t.id = "toast";
        t.className = "toast";
        document.body.appendChild(t);
        return t;
      })();
    el.textContent = msg;
    el.classList.add("show");
    clearTimeout(el._timer);
    el._timer = setTimeout(function () { el.classList.remove("show"); }, 2200);
  }

  function initDecision(id, labels) {
    renderBar(id, labels);
    var chosen = getChoice(id);
    if (chosen) {
      var d = DECISIONS[decisionIndex(id)];
      if (d.mode === "choose") {
        document.querySelectorAll(".variant").forEach(function (card, i) {
          if (String.fromCharCode(65 + i) === chosen) card.classList.add("v-chosen");
        });
      } else if (chosen === CONFIRMED) {
        var main = document.querySelector(".variant");
        if (main) main.classList.add("v-chosen");
      }
    }
  }

  return {
    DECISIONS: DECISIONS,
    getChoice: getChoice, setChoice: setChoice,
    renderBar: renderBar, renderIndex: renderIndex,
    renderSeatGrid: renderSeatGrid,
    initDecision: initDecision, toast: toast, baseDir: baseDir
  };
})();
