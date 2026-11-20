import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { normalizeShortcut, ShortcutInput, shortcutFromEvent } from "./ShortcutInput";

describe("ShortcutInput", () => {
  it("normalisiert Windows-Shortcut-Namen", () => {
    expect(normalizeShortcut("Shift+Control+KeyW")).toBe("CTRL+SHIFT+W");
    expect(normalizeShortcut("Alt+Digit1")).toBe("ALT+1");
  });

  it("erfasst und löscht Tastenkürzel per Tastatur", () => {
    const onChange = vi.fn();
    render(<ShortcutInput aria-label="Shortcut" value="Ctrl+K" onChange={onChange} />);
    const input = screen.getByRole("textbox", { name: "Shortcut" });
    fireEvent.keyDown(input, { key: "P", ctrlKey: true, shiftKey: true });
    expect(onChange).toHaveBeenLastCalledWith("Ctrl+Shift+P");
    fireEvent.keyDown(input, { key: "Delete" });
    expect(onChange).toHaveBeenLastCalledWith("");
  });

  it("ignoriert allein gedrückte Modifizierer", () => {
    expect(shortcutFromEvent({ key: "Control" } as React.KeyboardEvent<HTMLInputElement>)).toBeUndefined();
  });
});
