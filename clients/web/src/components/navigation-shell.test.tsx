import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { CatalogOption } from "../api/types";
import { createTranslator } from "../i18n/messages";
import { ContextBar } from "./ContextBar";
import { FirstRunChecklist, type FirstRunProgress } from "./FirstRunChecklist";
import { SaveAsClassDialog } from "./SaveAsClassDialog";
import { Sidebar } from "./Sidebar";

const t = createTranslator("zh-CN");

const FORMATS: CatalogOption[] = [
  { id: "print", name: { "zh-CN": "打印页", en: "Print page" }, description: { "zh-CN": "", en: "" } },
  { id: "pdf", name: { "zh-CN": "PDF", en: "PDF" }, description: { "zh-CN": "", en: "" } },
];

describe("Sidebar", () => {
  it("renders the three sections and highlights the active content view", () => {
    render(
      <Sidebar
        activeView="rules"
        context={{ kind: "temp" }}
        connection="demo"
        projects={[]}
        sessionClasses={[]}
        t={t}
        onSelectView={() => undefined}
        onSelectClass={() => undefined}
        onSelectTemp={() => undefined}
      />,
    );

    expect(screen.getByRole("heading", { name: "我的班级" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "班级内容" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "任务" })).toBeInTheDocument();

    const rules = screen.getByRole("button", { name: "规则与目标" });
    expect(rules).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: /临时工作台/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("lists server projects and session classes as class contexts", () => {
    render(
      <Sidebar
        activeView="roster"
        context={{ kind: "class", id: "p1", name: "初二（1）班" }}
        connection="local"
        projects={[
          { name: "初二（1）班", path: "p1", modified_at: "2026-08-01" },
        ]}
        sessionClasses={[{ id: "s1", name: "高一（5）班" }]}
        t={t}
        onSelectView={() => undefined}
        onSelectClass={() => undefined}
        onSelectTemp={() => undefined}
      />,
    );

    const project = screen.getByRole("button", { name: /初二（1）班/ });
    expect(project).toHaveAttribute("data-active", "true");
    expect(screen.getByRole("button", { name: /高一（5）班/ })).toBeInTheDocument();
  });

  it("shows the empty hint while the service is disconnected", () => {
    render(
      <Sidebar
        activeView="roster"
        context={{ kind: "temp" }}
        connection="demo"
        projects={[]}
        sessionClasses={[]}
        t={t}
        onSelectView={() => undefined}
        onSelectClass={() => undefined}
        onSelectTemp={() => undefined}
      />,
    );

    expect(screen.getByText(/连接本地服务后/)).toBeInTheDocument();
  });

  it("does not leave the class section blank when the local list is empty", () => {
    render(
      <Sidebar
        activeView="roster"
        context={{ kind: "temp" }}
        connection="local"
        projects={[]}
        sessionClasses={[]}
        t={t}
        onSelectView={() => undefined}
        onSelectClass={() => undefined}
        onSelectTemp={() => undefined}
      />,
    );

    expect(screen.getByText(/还没有保存的班级/)).toBeInTheDocument();
  });

  it("notifies navigation targets", async () => {
    const user = userEvent.setup();
    const onView = vi.fn();
    const onClass = vi.fn();
    const onTemp = vi.fn();

    render(
      <Sidebar
        activeView={null}
        context={{ kind: "class", id: "p1", name: "初二（1）班" }}
        connection="local"
        projects={[{ name: "初二（1）班", path: "p1", modified_at: "" }]}
        sessionClasses={[]}
        t={t}
        onSelectView={onView}
        onSelectClass={onClass}
        onSelectTemp={onTemp}
      />,
    );

    await user.click(screen.getByRole("button", { name: "学生名单" }));
    expect(onView).toHaveBeenCalledWith("roster");
    await user.click(screen.getByRole("button", { name: /初二（1）班/ }));
    expect(onClass).toHaveBeenCalledWith("p1", "初二（1）班");
    await user.click(screen.getByRole("button", { name: /临时工作台/ }));
    expect(onTemp).toHaveBeenCalled();
  });
});

describe("ContextBar", () => {
  const base = {
    context: { kind: "temp" } as const,
    viewLabel: "名单",
    meta: null,
    exportFormats: FORMATS,
    locale: "zh-CN" as const,
    isGenerating: false,
    canGenerate: true,
    t,
    onAction: vi.fn(),
    onQuickExport: vi.fn(),
    onExportSettings: vi.fn(),
    onSaveAsClass: vi.fn(),
  };

  it("offers the next step for content views", () => {
    render(
      <ContextBar
        {...base}
        viewLabel="规则与目标"
        action={{ kind: "navigate", target: "generate", label: "ctx.nextGenerate" }}
      />,
    );
    expect(
      screen.getByRole("button", { name: /生成方案/ }),
    ).toBeInTheDocument();
  });

  it("opens the quick export menu with formats and settings entry", async () => {
    const user = userEvent.setup();
    render(<ContextBar {...base} viewLabel="调整" action={{ kind: "exportMenu" }} />);

    await user.click(screen.getByRole("button", { name: /导出/ }));
    await user.click(screen.getByRole("menuitem", { name: "PDF" }));
    expect(base.onQuickExport).toHaveBeenCalledWith("pdf");

    await user.click(screen.getByRole("button", { name: /导出/ }));
    await user.click(screen.getByRole("menuitem", { name: "版式与隐私设置" }));
    expect(base.onExportSettings).toHaveBeenCalled();
  });

  it("offers save-as-class only in the scratch workspace", () => {
    const { rerender } = render(<ContextBar {...base} action={{ kind: "exportMenu" }} />);
    expect(screen.getByRole("button", { name: "另存为班级" })).toBeInTheDocument();

    rerender(
      <ContextBar
        {...base}
        context={{ kind: "class", id: "p1", name: "初二（1）班" }}
        action={{ kind: "exportMenu" }}
      />,
    );
    expect(screen.queryByRole("button", { name: "另存为班级" })).toBeNull();
  });

  it("disables generation while running or when the roster is invalid", () => {
    const { rerender } = render(
      <ContextBar
        {...base}
        action={{ kind: "generate", label: "action.generate" }}
      />,
    );
    expect(screen.getByRole("button", { name: /生成座位表/ })).toBeEnabled();

    rerender(
      <ContextBar
        {...base}
        isGenerating
        action={{ kind: "generate", label: "action.generate" }}
      />,
    );
    expect(screen.getByRole("button", { name: /生成座位表/ })).toBeDisabled();
  });
});

describe("FirstRunChecklist", () => {
  it("ticks completed steps and stays unticked otherwise", () => {
    const progress: FirstRunProgress = {
      roster: true,
      room: true,
      rules: false,
      generate: false,
      export: false,
    };
    render(
      <FirstRunChecklist progress={progress} t={t} onDismiss={() => undefined} />,
    );

    expect(screen.getByText("导入名单").closest("li")).toHaveAttribute(
      "data-done",
      "true",
    );
    expect(screen.getByText("设置规则").closest("li")).toHaveAttribute(
      "data-done",
      "false",
    );
  });

  it("notifies dismissal", async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(
      <FirstRunChecklist
        progress={{ roster: false, room: false, rules: false, generate: false, export: false }}
        t={t}
        onDismiss={onDismiss}
      />,
    );

    await user.click(screen.getByRole("button", { name: "知道了" }));
    expect(onDismiss).toHaveBeenCalled();
  });
});

describe("SaveAsClassDialog", () => {
  it("confirms a non-empty class name and closes on cancel", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onClose = vi.fn();

    render(
      <SaveAsClassDialog open t={t} onClose={onClose} onConfirm={onConfirm} />,
    );

    const confirm = screen.getByRole("button", { name: "保存并进入" });
    expect(confirm).toBeDisabled();

    await user.type(screen.getByLabelText("班级名称"), "初二（3）班");
    await user.click(confirm);
    expect(onConfirm).toHaveBeenCalledWith("初二（3）班");

    render(
      <SaveAsClassDialog open t={t} onClose={onClose} onConfirm={onConfirm} />,
    );
    await user.click(screen.getAllByRole("button", { name: "关闭" })[0]);
    expect(onClose).toHaveBeenCalled();
  });

  it("renders nothing while closed", () => {
    const { container } = render(
      <SaveAsClassDialog open={false} t={t} onClose={() => undefined} onConfirm={() => undefined} />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
