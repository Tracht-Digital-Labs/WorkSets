import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { expectAccessible } from "../test/accessibility";
import { presetFixture, reportFixture } from "../test/fixtures";
import { Dashboard } from "./Dashboard";

describe("Dashboard", () => {
  it("zeigt Preset-Gesundheit und aktiviert per Tastatur", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    const onApply = vi.fn();
    render(
      <Dashboard
        presets={[preset]}
        lastApplyReport={reportFixture()}
        onCreate={vi.fn()}
        onCapture={vi.fn()}
        onEdit={vi.fn()}
        onDuplicate={vi.fn().mockResolvedValue(undefined)}
        onDelete={vi.fn().mockResolvedValue(undefined)}
        onApply={onApply}
        onExport={vi.fn().mockResolvedValue(undefined)}
        onImport={vi.fn()}
        onOpenUpdate={vi.fn()}
        onDismissUpdate={vi.fn()}
      />,
    );

    expect(screen.getByRole("heading", { name: "Fokus" })).toBeVisible();
    expect(screen.getByText("1 App")).toBeVisible();
    expect(screen.getByText("Bereit")).toBeVisible();
    await expectAccessible();

    const apply = screen.getByRole("button", { name: "Aktivieren" });
    apply.focus();
    await user.keyboard("{Enter}");
    expect(onApply).toHaveBeenCalledWith(preset);
  });
});
