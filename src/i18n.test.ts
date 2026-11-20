import i18n, { i18nResources } from "./i18n";

function keys(value: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return child && typeof child === "object" ? keys(child as Record<string, unknown>, path) : [path];
  });
}

describe("i18n", () => {
  it("hält Deutsch und Englisch vollständig deckungsgleich", () => {
    expect(keys(i18nResources.de.translation).sort()).toEqual(keys(i18nResources.en.translation).sort());
  });

  it("übersetzt Pluralformen und Interpolation", async () => {
    await i18n.changeLanguage("en");
    expect(i18n.t("dashboard.apps", { count: 2 })).toBe("2 apps");
    expect(i18n.t("update.title", { version: "0.2.0" })).toContain("0.2.0");
  });
});
