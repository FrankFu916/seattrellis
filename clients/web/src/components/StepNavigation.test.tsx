import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createTranslator } from "../i18n/messages";
import { StepNavigation } from "./StepNavigation";

describe("StepNavigation", () => {
  it("announces the current step and supports direct navigation", async () => {
    const user = userEvent.setup();
    const handleChange = vi.fn();

    render(
      <StepNavigation
        activeStep="room"
        t={createTranslator("en")}
        onStepChange={handleChange}
      />,
    );

    expect(screen.getByRole("button", { name: "Room" })).toHaveAttribute(
      "aria-current",
      "step",
    );
    await user.click(screen.getByRole("button", { name: "Goal" }));
    expect(handleChange).toHaveBeenCalledWith("goal");
  });
});

