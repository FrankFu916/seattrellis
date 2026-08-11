/* ---------------------------------------------------------------------------
 * SeatTrellis v2 M4 prototype gallery — shared behaviour.
 *
 * Responsibilities:
 *   - decision registry (D1..D8) with titles, paths and variant names;
 *   - choice persistence in localStorage (key `seattrellis-prototypes:v1`);
 *   - sticky decision bar (pick A/B/C/D or "redesign", prev/next navigation);
 *   - index page progress + summary export;
 *   - reusable seat-grid renderer used by several mockup pages.
 * ------------------------------------------------------------------------- */

window.PTUI = (function () {
  "use strict";

  var STORAGE_KEY = "seattrellis-prototypes:v1";

  // Paths are relative to docs/prototypes/ (index) or docs/prototypes/decisions/.
  var DECISIONS = [
    { id: "d1", n: 1, title: "班级主导航",        file: "decisions/d1-navigation.html",
      variants: ["单页工作台", "步骤向导", "侧栏工作区"] },
    { id: "d2", n: 2, title: "Seating canvas 交互", file: "decisions/d2-canvas.html",
      variants: ["缩放 + 拖拽", "框选 + 批量", "表格直编"] },
    { id: "d3", n: 3, title: "Rule builder",     file: "decisions/d3-rule-builder.html",
      variants: ["句式构建器", "规则卡片", "分层表单"] },
    { id: "d4", n: 4, title: "快速 vs 高级模式",   file: "decisions/d4-quick-advanced.html",
      variants: ["默认精简", "默认展开"] },
    { id: "d5", n: 5, title: "候选方案比较",       file: "decisions/d5-candidates.html",
      variants: ["对比表格", "雷达图", "差异高亮", "逐规则列表"] },
    { id: "d6", n: 6, title: "可行性诊断",         file: "decisions/d6-diagnostics.html",
      variants: ["问题列表 + 高亮", "向导式修复", "内联诊断"] },
    { id: "d7", n: 7, title: "历史 / 轮换模型",    file: "decisions/d7-history-rotation.html",
      variants: ["时间线", "版本列表", "周期计划"] },
    { id: "d8", n: 8, title: "导入确认流程",       file: "decisions/d8-import.html",
      variants: ["三步向导", "单页预览", "内联确认"] }
  ];

  var REDESIGN = { key: "R", label: "都不满意，重新设计" };

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

  /* ---------- decision bar ---------- */

  function decisionIndex(id) {
    for (var i = 0; i < DECISIONS.length; i++) {
      if (DECISIONS[i].id === id) return i;
    }
    return -1;
  }

  function baseDir() {
    // decision pages live under decisions/; index lives at the root.
    return /\/decisions\//.test(location.pathname) ? "../" : "";
  }

  function renderDecisionBar(id, labels) {
    var bar = document.getElementById("decisionBar");
    if (!bar) return;
    var d = DECISIONS[decisionIndex(id)];
    var idx = decisionIndex(id);
    var prev = idx > 0 ? DECISIONS[idx - 1] : null;
    var next = idx < DECISIONS.length - 1 ? DECISIONS[idx + 1] : null;
    var chosen = getChoice(id);

    var html = '<span class="db-label">决策 <b>D' + d.n + '</b> · ' + d.title + ' — 选择你偏好的方案：</span>';
    labels.forEach(function (label, i) {
      var key = String.fromCharCode(65 + i); // A, B, C, D
      var picked = chosen === key;
      html += '<button class="db-btn' + (picked ? " picked" : "") + '" data-choice="' + key + '">' +
        '<span class="key">' + key + "</span>" + label + "</button>";
    });
    var pickedR = chosen === REDESIGN.key;
    html += '<button class="db-btn redesign' + (pickedR ? " picked" : "") + '" data-choice="' + REDESIGN.key + '">' +
      '<span class="key">R</span>' + REDESIGN.label + "</button>";
    html += '<span class="db-sep"></span>';
    html += '<a class="db-nav" href="' + baseDir() + 'index.html">← 决策总览</a>';
    if (prev) html += '<a class="db-nav" href="' + prev.file + '">← D' + prev.n + "</a>";
    if (next) html += '<a class="db-nav" href="' + next.file + '">D' + next.n + " →</a>";
    bar.innerHTML = html;

    bar.querySelectorAll(".db-btn").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var choice = btn.getAttribute("data-choice");
        setChoice(id, choice);
        renderDecisionBar(id, labels); // re-render to move the highlight
        toast(choice === REDESIGN.key
          ? "已记录：对 " + d.title + " 不满意，希望重新设计"
          : "已选择方案 " + choice + "（" + labels[choice.charCodeAt(0) - 65] + "）");
        // Highlight the corresponding variant card.
        document.querySelectorAll(".variant").forEach(function (card, i) {
          card.classList.toggle("v-chosen", String.fromCharCode(65 + i) === choice);
        });
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
      var stateHtml = choice
        ? '<span class="pick">已选 ' + choice + " — " + (choice === REDESIGN.key ? REDESIGN.label : d.variants[choice.charCodeAt(0) - 65]) + "</span>"
        : '<span class="none">尚未选择</span>';
      card.innerHTML =
        '<div class="ic-top"><span class="v-badge">D' + d.n + "</span>" +
        '<span class="ic-title">' + d.title + "</span></div>" +
        '<div class="ic-sub">' + d.variants.join(" / ") + "</div>" +
        '<div class="ic-state">' + stateHtml + "</div>";
      grid.appendChild(card);
    });
    var pct = Math.round((done / DECISIONS.length) * 100);
    document.getElementById("idxFill").style.width = pct + "%";
    document.getElementById("idxPct").textContent = pct + "%";
    document.getElementById("idxDone").textContent = done + " / " + DECISIONS.length + " 项已决策";
    document.getElementById("idxHint").textContent =
      pct === 100 ? "全部决策完成 — 复制下方摘要作为 M4 决策记录初稿。" :
      "打开每个决策页，比较变体后点底部选择按钮。选择会保存在本浏览器。";
    renderSummary(state);
  }

  function renderSummary(state) {
    var lines = ["# SeatTrellis v2 M4 — Product Decision Summary", "",
      "Source: docs/prototypes static gallery (real fixture roster data).", ""];
    DECISIONS.forEach(function (d) {
      var choice = state[d.id];
      var line = choice
        ? "- D" + d.n + " " + d.title + ": **" + choice + "** — " +
          (choice === REDESIGN.key ? REDESIGN.label : d.variants[choice.charCodeAt(0) - 65])
        : "- D" + d.n + " " + d.title + ": _pending_";
      lines.push(line);
    });
    lines.push("", "Note: a \"redesign\" choice opens the decision again with new variants.");
    document.getElementById("idxSummaryText").value = lines.join("\n");
  }

  /* ---------- shared seat-grid renderer ---------- */

  // Options: { plan: {assignment}, locks: [seatIds], violations: [seatIds],
  //           selected: [seatIds], dimmed: [seatIds], tags: {seatId: text},
  //           highlightDiff: {seatId: "up"|"down"} }
  function renderSeatGrid(container, opts) {
    var plan = opts.plan || null;
    var locks = opts.locks || [];
    var violations = opts.violations || [];
    var selected = opts.selected || [];
    var dimmed = opts.dimmed || [];
    var tags = opts.tags || {};
    var diff = opts.diff || {};

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

  function renderPlanRows(plan, extra) {
    var html = "";
    for (var r = 1; r <= PT.rows; r++) {
      var cells = "";
      for (var c = 1; c <= PT.cols; c++) {
        var sid = "R" + r + "C" + c;
        var student = plan.assignment[sid];
        var st = student ? PT.byId[student] : null;
        cells += "<td>" + (st ? st.name : "—") + (extra && extra[sid] ? " " + extra[sid] : "") + "</td>";
      }
      html += "<tr><th>第 " + r + " 排</th>" + cells + "</tr>";
    }
    return html;
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
    renderDecisionBar(id, labels);
    var chosen = getChoice(id);
    if (chosen) {
      document.querySelectorAll(".variant").forEach(function (card, i) {
        if (String.fromCharCode(65 + i) === chosen) card.classList.add("v-chosen");
      });
    }
  }

  return {
    DECISIONS: DECISIONS, REDESIGN: REDESIGN,
    getChoice: getChoice, setChoice: setChoice,
    renderDecisionBar: renderDecisionBar, renderIndex: renderIndex,
    renderSeatGrid: renderSeatGrid, renderPlanRows: renderPlanRows,
    initDecision: initDecision, toast: toast, baseDir: baseDir
  };
})();
