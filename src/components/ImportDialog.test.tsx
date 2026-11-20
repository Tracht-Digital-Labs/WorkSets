import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { worksetsApi } from "../api";
import { expectAccessible } from "../test/accessibility";
import { presetFixture } from "../test/fixtures";
import { ImportDialog } from "./ImportDialog";

describe("ImportDialog", () => {
  it("zeigt abgelehnte native Importfehler im Dialog an", async () => {
    const user = userEvent.setup();
    vi.spyOn(worksetsApi, "importPreset").mockRejectedValue(new Error("Schema 9 wird nicht unterstützt."));

    render(<ImportDialog open onClose={vi.fn()} onImported={vi.fn()} />);
    await user.click(screen.getByRole("textbox", { name: "Preset-JSON einfügen" }));
    await user.paste("{\"schemaVersion\":9}");
    await user.click(screen.getByRole("button", { name: "Import prüfen" }));

    expect(await screen.findByText(/Schema 9 wird nicht unterstützt/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Import prüfen" })).toBeEnabled();
    await expectAccessible();
  });

  it("zeigt Import- und Migrationshinweise lokalisiert in der Vorschau", async () => {
    const user = userEvent.setup();
    vi.spyOn(worksetsApi, "importPreset").mockResolvedValue({
      valid: true,
      preset: presetFixture(),
      committed: false,
      errors: [],
      warnings: [{
        code: "presetSchemaMigrated",
        message: "Preset schema was migrated.",
        severity: "warning",
      }],
    });

    render(<ImportDialog open onClose={vi.fn()} onImported={vi.fn()} />);
    await user.click(screen.getByRole("textbox", { name: "Preset-JSON einfügen" }));
    await user.paste("{\"schemaVersion\":0}");
    await user.click(screen.getByRole("button", { name: "Import prüfen" }));

    expect(await screen.findByText("Das Preset wurde auf die aktuelle Schema-Version migriert.")).toBeVisible();
    expect(screen.queryByText("Preset schema was migrated.")).not.toBeInTheDocument();
  });
});
