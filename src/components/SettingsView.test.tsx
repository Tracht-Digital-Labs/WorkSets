import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { expectAccessible } from "../test/accessibility";
import { presetFixture, settingsFixture } from "../test/fixtures";
import { SettingsView } from "./SettingsView";

describe("SettingsView", () => {
  it("erkennt Hotkey-Konflikte, löst sie auf und wechselt vollständig auf Englisch", async () => {
    const user = userEvent.setup();
    const preset = presetFixture({ hotkey: "Ctrl+Shift+W" });
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<SettingsView settings={settingsFixture()} presets={[preset]} onSave={onSave} onOpenUpdate={vi.fn()} />);

    expect(screen.getByText("Dieses Tastenkürzel ist bereits einem Preset zugewiesen.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Speichern" })).toBeDisabled();

    const shortcut = screen.getByRole("textbox", { name: "Tastenkürzel zum Erfassen" });
    fireEvent.keyDown(shortcut, { key: "K", ctrlKey: true, altKey: true });
    expect(shortcut).toHaveValue("Ctrl+Alt+K");
    expect(screen.queryByText("Dieses Tastenkürzel ist bereits einem Preset zugewiesen.")).not.toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Sprache" }), "en");
    expect(screen.getByRole("heading", { name: "Settings" })).toBeVisible();
    await expectAccessible();

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ language: "en", captureHotkey: "Ctrl+Alt+K" }));
  });
});
