import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { worksetsApi } from "../api";
import { expectAccessible } from "../test/accessibility";
import { presetFixture } from "../test/fixtures";
import { CaptureDialog } from "./CaptureDialog";

describe("CaptureDialog", () => {
  it("filtert unverwaltbare Fenster und validiert die Auswahl", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    const elevated = {
      ...preset.items[0],
      id: "22222222-2222-4222-8222-222222222222",
      displayName: "Admin-Terminal",
      elevated: true,
      enabled: false,
    };
    vi.spyOn(worksetsApi, "captureWorkspace").mockResolvedValue({
      preset: { ...preset, items: [...preset.items, elevated] },
      warnings: [{
        code: "elevatedWindowCaptured",
        message: "One elevated window cannot be managed.",
        severity: "warning",
      }],
    });
    const onContinue = vi.fn();
    render(<CaptureDialog open onClose={vi.fn()} onContinue={onContinue} />);

    expect(await screen.findByRole("checkbox", { name: "Editor" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Admin-Terminal" })).toBeDisabled();
    expect(screen.getByText("Ein Fenster mit Administratorrechten kann nur erfasst, aber nicht verwaltet werden.")).toBeVisible();
    expect(screen.queryByText("One elevated window cannot be managed.")).not.toBeInTheDocument();
    await expectAccessible();

    await user.click(screen.getByRole("button", { name: "Auswahl aufheben" }));
    await user.click(screen.getByRole("button", { name: "Im Editor anpassen" }));
    expect(screen.getByText("Wähle mindestens ein Fenster aus.")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Alle auswählbaren Fenster" }));
    await user.clear(screen.getByRole("textbox", { name: /Preset-Name/ }));
    await user.type(screen.getByRole("textbox", { name: /Preset-Name/ }), "  Neuer Fokus  ");
    await user.click(screen.getByRole("button", { name: "Im Editor anpassen" }));

    await waitFor(() => expect(onContinue).toHaveBeenCalled());
    const captured = onContinue.mock.calls[0][0];
    expect(captured.name).toBe("Neuer Fokus");
    expect(captured.items).toHaveLength(1);
    expect(captured.items[0].displayName).toBe("Editor");
  });
});
