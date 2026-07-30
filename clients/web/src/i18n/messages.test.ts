import { createTranslator, supportedLocales } from "./messages";

describe("translations", () => {
  it.each(supportedLocales)("interpolates values in %s", (locale) => {
    const t = createTranslator(locale);

    expect(t("app.students", { count: 18 })).toContain("18");
    expect(t("canvas.seatLabel", {
      row: 2,
      column: 3,
      student: "Mina",
      locked: "",
    })).toContain("Mina");
  });
});

