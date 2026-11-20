import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { expectAccessible } from "../test/accessibility";
import { presetFixture } from "../test/fixtures";
import { PresetEditor } from "./PresetEditor";

describe("PresetEditor", () => {
  // Includes an axe accessibility pass, which alone can exceed 5 s on a loaded runner.
  it("verschiebt Fenster per Pfeiltaste und schützt ungespeicherte Änderungen", async () => {
    const user = userEvent.setup();
    render(<PresetEditor initialPreset={presetFixture()} onSave={vi.fn().mockResolvedValue(undefined)} onCancel={vi.fn()} />);

    const windowCard = screen.getByRole("button", { name: /^Editor, 100, 120/ });
    await user.click(windowCard);
    fireEvent.keyDown(windowCard, { key: "ArrowRight" });
    await waitFor(() => expect(screen.getByRole("spinbutton", { name: "X" })).toHaveValue(101));
    await expectAccessible();

    await user.click(screen.getByRole("button", { name: "Zurück" }));
    // Locate the dialog through its title: under load jsdom can apply tabster's modal aria-hidden
    // to the surface itself, which hides it from role queries although it is open and visible.
    const title = await screen.findByText("Ungespeicherte Änderungen", {}, { timeout: 5000 });
    const dialog = title.closest('[role="dialog"]');
    expect(dialog).toHaveAttribute("aria-labelledby", title.id);
    expect(dialog).toBeVisible();
    expect(screen.getByText("Möchtest du deine Änderungen verwerfen?")).toBeVisible();
  }, 20_000);

  it("legt ein valides Programm an und lässt den Zieltyp per Tastatur ändern", async () => {
    const user = userEvent.setup();
    const initial = presetFixture({ name: "Neues Setup", description: "", items: [] });
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<PresetEditor initialPreset={initial} onSave={onSave} onCancel={vi.fn()} />);

    const add = screen.getByRole("button", { name: "Programm oder Ziel hinzufügen" });
    add.focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("heading", { name: "Neue App" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Pfad oder Adresse" })).toHaveValue("C:\\Windows\\System32\\notepad.exe");

    await user.selectOptions(screen.getByRole("combobox", { name: "Typ" }), "url");
    expect(screen.getByRole("textbox", { name: "Pfad oder Adresse" })).toHaveValue("https://example.com");
    await user.clear(screen.getByRole("textbox", { name: "Anzeigename" }));
    await user.type(screen.getByRole("textbox", { name: "Anzeigename" }), "Dokumentation");
    await expectAccessible();

    await user.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.items[0]).toMatchObject({
      displayName: "Dokumentation",
      launchTarget: { kind: "url", url: "https://example.com" },
      selector: { executablePath: undefined, aumid: undefined },
      enabled: true,
    });
  });

  it("ordnet eine auf den Primärmonitor gezogene Karte neu zu und normalisiert gegen dessen Arbeitsfläche", async () => {
    const user = userEvent.setup();
    const initial = presetFixture();
    const left = initial.monitors.find((monitor) => !monitor.primary);
    if (!left) throw new Error("Secondary monitor fixture is missing");
    initial.items[0].placement = {
      ...initial.items[0].placement,
      monitorId: left.id,
      x: -1800,
      y: 230,
      width: 600,
      height: 500,
      normalizedX: (-1800 - left.workArea.x) / left.workArea.width,
      normalizedY: (230 - left.workArea.y) / left.workArea.height,
      normalizedWidth: 600 / left.workArea.width,
      normalizedHeight: 500 / left.workArea.height,
    };
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { container } = render(<PresetEditor initialPreset={initial} onSave={onSave} onCancel={vi.fn()} />);
    const canvas = container.querySelector<HTMLElement>(".monitor-canvas");
    if (!canvas) throw new Error("Monitor canvas was not rendered");
    vi.spyOn(canvas, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 1000,
      bottom: 321,
      width: 1000,
      height: 321,
      toJSON: () => undefined,
    });
    const card = screen.getByRole("button", { name: /^Editor, -1800, 230/ });

    fireEvent.pointerDown(card, { button: 0, pointerId: 1, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(canvas, { pointerId: 1, clientX: 502, clientY: 100 });
    fireEvent.pointerUp(canvas, { pointerId: 1 });
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());

    const moved = onSave.mock.calls[0][0].items[0];
    const primary = initial.monitors.find((monitor) => monitor.primary);
    if (!primary) throw new Error("Primary monitor fixture is missing");
    expect(moved.placement.monitorId).toBe(primary.id);
    expect(Math.round(primary.workArea.x + moved.placement.normalizedX * primary.workArea.width)).toBe(moved.placement.x);
    expect(Math.round(primary.workArea.y + moved.placement.normalizedY * primary.workArea.height)).toBe(moved.placement.y);
    expect(Math.round(moved.placement.normalizedWidth * primary.workArea.width)).toBe(moved.placement.width);
    expect(Math.round(moved.placement.normalizedHeight * primary.workArea.height)).toBe(moved.placement.height);
  });

  it("überträgt einen Inspector-Monitorwechsel sofort auf dieselbe normalisierte Arbeitsflächenposition", async () => {
    const user = userEvent.setup();
    const initial = presetFixture();
    const source = initial.monitors.find((monitor) => monitor.primary);
    const target = initial.monitors.find((monitor) => !monitor.primary);
    if (!source || !target) throw new Error("Two-monitor fixture is missing");
    const sourcePlacement = structuredClone(initial.items[0].placement);
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<PresetEditor initialPreset={initial} onSave={onSave} onCancel={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: /^Editor, 100, 120/ }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Monitor" }), target.id);
    expect((screen.getByRole("spinbutton", { name: "X" }) as HTMLInputElement).valueAsNumber).toBeLessThan(0);
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());

    const moved = onSave.mock.calls[0][0].items[0];
    expect(moved.placement.monitorId).toBe(target.id);
    expect(moved.placement.normalizedX).toBeCloseTo((sourcePlacement.x - source.workArea.x) / source.workArea.width, 3);
    expect(moved.placement.normalizedY).toBeCloseTo((sourcePlacement.y - source.workArea.y) / source.workArea.height, 3);
    expect(moved.placement.x).toBeGreaterThanOrEqual(target.workArea.x);
    expect(moved.placement.x + moved.placement.width).toBeLessThanOrEqual(target.workArea.x + target.workArea.width);
    expect(moved.placement.y).toBeGreaterThanOrEqual(target.workArea.y);
    expect(moved.placement.y + moved.placement.height).toBeLessThanOrEqual(target.workArea.y + target.workArea.height);
  });
});
