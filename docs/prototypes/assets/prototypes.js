/* ---------------------------------------------------------------------------
 * SeatTrellis v2 M4 prototype gallery — shared behaviour (merged-plan flow).
 *
 * Batch-1 decisions are closed (see docs/product-decisions/
 * 2026-08-10-batch1-core-workflow.md). Each decision page now shows the
 * MERGED target form instead of competing variants; the footer bar offers
 * "confirm" / "suggest changes" and the index tracks confirmation state.
 * ------------------------------------------------------------------------- */

window.PTUI = (function () {
  "use strict";

  var STORAGE_KEY = "seattrellis-prototypes:v1";

  // Paths are relative to docs/prototypes/ (index) or docs/prototypes/decisions/.
  var DECISIONS = [
    { id: "d1", n: 1, title: "班级主导航", file: "decisions/d1-navigation.html",
      merged: "侧栏骨架 + 上下文操作条 + 临时工作台" },
    { id: "d2", n: 2, title: "Seating canvas 交互", file: "decisions/d2-canvas.html",
      merged: "画布主视图（拖拽整块跟随 + 框选）+ 表格视图并存" },
    { id: "d3", n: 3, title: "Rule builder", file: "decisions/d3-rule-builder.html",
      merged: "句式构建器（创建）+ 规则卡片（管理）+ 高级表单兜底" },
    { id: "d4", n: 4, title: "快速 vs 高级模式", file: "decisions/d4-quick-advanced.html",
      merged: "默认精简 + 历史范围可见 + 高级折叠区" },
    { id: "d5", n: 5, title: "候选方案比较", file: "decisions/d5-candidates.html",
      merged: "通俗推荐理由 + 差异高亮 + 明细切换（去术语化）" },
    { id: "d6", n: 6, title: "可行性诊断", file: "decisions/d6-diagnostics.html",
      merged: "内联徽章 + 问题列表双向联动 + 一键修复" },
    { id: "d7", n: 7, title: "历史 / 轮换模型", file: "decisions/d7-history-rotation.html",
      merged: "历史回顾（时间线）+ 轮换计划 双视图" },
    { id: "d8", n: 8, title: "导入确认流程", file: "decisions/d8-import.html",
      merged: "步骤条 + 映射/预览同屏（班级上下文内面板）" }
  ];

  var CONFIRMED = "confirmed";
  var REVISE = "revise";

  /* ---------- persistence ---------- */

  function load() {
    try {
      return JSON.parse(localStorage.getItem(STORAGE_KEY)) || {};
    } catch (e) {
      return {};
    }
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

  /* ---------- confirm bar ---------- */

  function decisionIndex(id) {
    for (var i = 0; i < DECISIONS.length; i++) {
      if (DECISIONS[i].id === id) return i;
    }
    return -1;
  }

  function baseDir() {
    return /\/decisions\//.test(location.pathname) ? "../" : "";
  }

  function renderConfirmBar(id) {
    var bar = document.getElementById("decisionBar");
    if (!bar) return;
    var d = DECISIONS[decisionIndex(id)];
    var idx = decisionIndex(id);
    var prev = idx > 0 ? DECISIONS[idx - 1] : null;
    var next = idx < DECISIONS.length - 1 ? DECISIONS[idx + 1] : null;
    var chosen = getChoice(id);

    var html = '<span class="db-label">决策 <b>D' + d.n + '</b> · ' + d.title +
      ' — 融合方案：<b style="color:#93c5fd">' + d.merged + "</b></span>";
    html += '<button class="db-btn confirm' + (chosen === CONFIRMED ? " picked" : "") +
      '" data-choice="' + CONFIRMED + '"><span class="key">✓</span>确认此方案</button>';
    html += '<button class="db-btn' + (chosen === REVISE ? " picked redesign" : "") +
      '" data-choice="' + REVISE + '"><span class="key">✎</span>有修改建议</button>';
    html += '<span class="db-sep"></span>';
    html += '<a class="db-nav" href="' + baseDir() + 'index.html">← 决策总览</a>';
    if (prev) html += '<a class="db-nav" href="' + prev.file + '">← D' + prev.n + "</a>";
    if (next) html += '<a class="db-nav" href="' + next.file + '">D' + next.n + " →</a>";
    bar.innerHTML = html;

    bar.querySelectorAll(".db-btn").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var choice = btn.getAttribute("data-choice");
        setChoice(id, choice);
        renderConfirmBar(id);
        toast(choice === CONFIRMED
          ? "已确认 D" + d.n + " 融合方案：" + d.merged
          : "已记录修改建议（D" + d.n + "）——融合方案将据此迭代");
        var main = document.querySelector(".variant");
        if (main) main.classList.toggle("v-chosen", choice === CONFIRMED);
      });
    });
  }

  /* ---------- index page ---------- */

  function renderIndex() {
    var state = load();
    var done = 0;
    var grid = document.getElementById("idxGrid");
    DECISIONS.forEach(function (d) {
      var choice = state[d.id];
      if (choice) done++;
      var card = document.createElement("a");
      card.className = "idx-card";
      card.href = d.file;
      var stateHtml;
      if (choice === CONFIRMED) {
        stateHtml = '<span class="pick">✓ 已确认</span>';
      } else if (choice === REVISE) {
        stateHtml = '<span class="pick" style="color:var(--warn)">✎ 有修改建议</span>';
      } else {
        stateHtml = '<span class="none">待确认</span>';
      }
      card.innerHTML =
        '<div class="ic-top"><span class="v-badge">D' + d.n + "</span>" +
        '<span class="ic-title">' + d.title + "</span></div>" +
        '<div class="ic-sub">融合方案：' + d.merged + "</div>" +
        '<div class="ic-state">' + stateHtml + "</div>";
      grid.appendChild(card);
    });
    var pct = Math.round((done / DECISIONS.length) * 100);
    document.getElementById("idxFill").style.width = pct + "%";
    document.getElementById("idxPct").textContent = pct + "%";
    document.getElementById("idxDone").textContent = done + " / " + DECISIONS.length + " 项已确认";
    document.getElementById("idxHint").textContent =
      pct === 100 ? "批 1 全部确认 — 决策记录见 docs/product-decisions/2026-08-10-batch1-core-workflow.md。" :
      "逐页查看融合方案，满意点「确认此方案」，不满意点「有修改建议」。";
    renderSummary(state);
  }

  function renderSummary(state) {
    var lines = ["# SeatTrellis v2 M4 — Product Decision Summary (batch 1)", "",
      "Source: docs/prototypes merged target forms; frozen record in",
      "docs/product-decisions/2026-08-10-batch1-core-workflow.md.", ""];
    DECISIONS.forEach(function (d) {
      var choice = state[d.id];
      var line;
      if (choice === CONFIRMED) line = "- D" + d.n + " " + d.title + ": **confirmed** — " + d.merged;
      else if (choice === REVISE) line = "- D" + d.n + " " + d.title + ": **changes requested** — " + d.merged;
      else line = "- D" + d.n + " " + d.title + ": _pending_ — " + d.merged;
      lines.push(line);
    });
    lines.push("", "Global decisions G-1..G-5 and per-item constraints: see the batch-1 record.");
    document.getElementById("idxSummaryText").value = lines.join("\n");
  }

  /* ---------- shared seat-grid renderer (unchanged) ---------- */

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

  function initDecision(id) {
    renderConfirmBar(id);
    if (getChoice(id) === CONFIRMED) {
      var main = document.querySelector(".variant");
      if (main) main.classList.add("v-chosen");
    }
  }

  return {
    DECISIONS: DECISIONS,
    getChoice: getChoice, setChoice: setChoice,
    renderConfirmBar: renderConfirmBar, renderIndex: renderIndex,
    renderSeatGrid: renderSeatGrid,
    initDecision: initDecision, toast: toast, baseDir: baseDir
  };
})();
