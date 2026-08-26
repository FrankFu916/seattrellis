import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { createTranslator } from "../i18n/messages";
import type { ClassContext } from "../domain/navigation";
import { ClassContextGuide } from "./ClassContextGuide";

describe("ClassContextGuide (W4: class context without loaded data)", () => {
  it("stays hidden in the scratch workspace", () => {
    const { container } = render(
      <ClassContextGuide
        context={{ kind: "temp" }}
        t={createTranslator("zh-CN")}
        onOpenTools={() => undefined}
      />,
    );

    expect(container).toBeEmptyDOMElement();
  });

  it("explains the scratch draft and offers the project tools for a class", async () => {
    const user = userEvent.setup();
    const onOpenTools = vi.fn();
    const context: ClassContext = {
      kind: "class",
      id: "/classes/demo.seattrellis.json",
      name: "初二（3）班",
    };
    render(
      <ClassContextGuide
        context={context}
        t={createTranslator("zh-CN")}
        onOpenTools={onOpenTools}
      />,
    );

    expect(screen.getByTestId("class-guide")).toHaveTextContent(
      "此班级尚未载入数据",
    );
    expect(screen.getByTestId("class-guide")).toHaveTextContent(
      "项目工具",
    );

    await user.click(
      screen.getByRole("button", { name: "打开项目工具" }),
    );
    expect(onOpenTools).toHaveBeenCalledOnce();
  });
});
