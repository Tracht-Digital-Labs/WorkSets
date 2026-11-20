import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { expectAccessible } from "../test/accessibility";
import { compatibilityFixture, settingsFixture } from "../test/fixtures";
import { Onboarding } from "./Onboarding";

describe("Onboarding", () => {
  it("führt vollständig per Tastatur durch die Einrichtung", async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn().mockResolvedValue(undefined);
    render(
      <Onboarding
        compatibility={compatibilityFixture()}
        settings={settingsFixture({ onboardingComplete: false })}
        onFinish={onFinish}
      />,
    );

    expect(screen.getByRole("heading", { name: "Dein Desktop. Genau so, wie du ihn brauchst." })).toBeVisible();
    await expectAccessible();

    await user.click(screen.getByRole("button", { name: "Fortfahren" }));
    const startup = screen.getByRole("switch", { name: "Beim Anmelden starten" });
    await user.click(startup);
    expect(startup).toBeChecked();

    const shortcut = screen.getByRole("textbox", { name: "Tastenkürzel zum Erfassen" });
    shortcut.focus();
    await user.keyboard("{Control>}{Alt>}r{/Alt}{/Control}");
    expect(shortcut).toHaveValue("Ctrl+Alt+R");

    await user.click(screen.getByRole("button", { name: "Fortfahren" }));
    await user.click(screen.getByRole("button", { name: "Erst einmal zur Übersicht" }));

    expect(onFinish).toHaveBeenCalledWith(
      expect.objectContaining({ onboardingComplete: true, runAtStartup: true, captureHotkey: "Ctrl+Alt+R" }),
      false,
    );
  });
});
