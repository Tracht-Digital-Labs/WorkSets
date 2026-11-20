import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { worksetsApi } from "../api";
import i18n from "../i18n";
import { expectAccessible } from "../test/accessibility";
import { presetFixture, reportFixture } from "../test/fixtures";
import { ApplyFlow } from "./ApplyFlow";

describe("ApplyFlow", () => {
  it("lässt sich schon während einer langsamen Planung per Tastatur schließen", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    vi.spyOn(worksetsApi, "planPresetApply").mockImplementation(() => new Promise(() => undefined));
    const onClose = vi.fn();

    render(<ApplyFlow preset={preset} onClose={onClose} onCompleted={vi.fn()} onDiagnostics={vi.fn()} />);

    const cancel = await screen.findByRole("button", { name: "Abbrechen" });
    await expectAccessible();
    cancel.focus();
    expect(cancel).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("startet einen risikofreien Plan ohne Routinebestätigung", async () => {
    const preset = presetFixture();
    vi.spyOn(worksetsApi, "planPresetApply").mockResolvedValue({
      presetId: preset.id,
      canApply: true,
      desktopChanges: 0,
      reusedWindows: 1,
      launchedWindows: 0,
      closedWindows: 0,
      adjustedWindows: 0,
      risks: [],
    });
    vi.spyOn(worksetsApi, "applyPreset").mockResolvedValue(reportFixture());

    render(<ApplyFlow preset={preset} onClose={vi.fn()} onCompleted={vi.fn()} onDiagnostics={vi.fn()} />);

    expect(await screen.findByRole("heading", { name: "Arbeitsplatz ist bereit" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Jetzt wechseln" })).not.toBeInTheDocument();
    expect(worksetsApi.applyPreset).toHaveBeenCalledWith(preset.id, {});
  });

  it("erzwingt blockierte Schließvorgänge erst nach einer ausdrücklichen zweiten Bestätigung", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    vi.spyOn(worksetsApi, "planPresetApply").mockResolvedValue({
      presetId: preset.id,
      canApply: true,
      desktopChanges: 0,
      reusedWindows: 1,
      launchedWindows: 0,
      closedWindows: 0,
      adjustedWindows: 0,
      risks: [],
    });
    const blocked = reportFixture({
      status: "partial",
      issues: [{
        id: "closeBlocked",
        severity: "warning",
        title: "closeBlocked",
        detail: "A window blocked the close request.",
      }],
    });
    vi.spyOn(worksetsApi, "applyPreset")
      .mockResolvedValueOnce(blocked)
      .mockResolvedValueOnce(reportFixture());

    render(<ApplyFlow preset={preset} onClose={vi.fn()} onCompleted={vi.fn()} onDiagnostics={vi.fn()} />);

    expect(await screen.findByText("Ein verwaltetes Fenster hat das normale Schließen blockiert.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Schließen erzwingen …" }));
    expect(screen.getByText(/Ungespeicherte Arbeit kann verloren gehen/)).toBeVisible();
    expect(worksetsApi.applyPreset).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Nur WorkSets-Prozesse erzwingen" }));
    expect(await screen.findByRole("heading", { name: "Arbeitsplatz ist bereit" })).toBeVisible();
    expect(worksetsApi.applyPreset).toHaveBeenLastCalledWith(preset.id, expect.objectContaining({ forceOwnedProcesses: true }));
  });

  it("blockiert einen riskanten Wechsel bis fremde Fenster geschützt sind", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    vi.spyOn(worksetsApi, "planPresetApply").mockResolvedValue({
      presetId: preset.id,
      canApply: true,
      desktopChanges: 1,
      reusedWindows: 1,
      launchedWindows: 0,
      closedWindows: 0,
      adjustedWindows: 0,
      risks: [{
        id: "foreign",
        code: "foreignWindowsOnRemovedDesktops",
        kind: "foreignWindows",
        title: "Zwei fremde Fenster",
        detail: "Die Fenster werden vor dem Entfernen verschoben.",
        blocking: true,
        affectedItemIds: [],
        count: 2,
      }],
    });
    vi.spyOn(worksetsApi, "applyPreset").mockResolvedValue(reportFixture());
    const onCompleted = vi.fn();
    render(<ApplyFlow preset={preset} onClose={vi.fn()} onCompleted={onCompleted} onDiagnostics={vi.fn()} />);

    const start = await screen.findByRole("button", { name: "Jetzt wechseln" });
    expect(start).toBeDisabled();
    expect(screen.getByText("2 fremde Fenster")).toBeVisible();
    expect(screen.getByText(/Fremde Fenster müssen vor dem Entfernen/)).toBeVisible();
    expect(screen.queryByText("Die Fenster werden vor dem Entfernen verschoben.")).not.toBeInTheDocument();
    await expectAccessible();

    await user.click(screen.getByRole("checkbox", { name: "Fremde Fenster auf einen verbleibenden Desktop verschieben" }));
    expect(start).toBeEnabled();
    await user.click(start);

    expect(await screen.findByRole("heading", { name: "Arbeitsplatz ist bereit" })).toBeVisible();
    await waitFor(() => expect(onCompleted).toHaveBeenCalledWith(expect.objectContaining({ status: "success" })));
    expect(worksetsApi.applyPreset).toHaveBeenCalledWith(preset.id, expect.objectContaining({ confirmForeignWindows: true }));
  });

  it("bestätigt experimentelle Desktop-Steuerung unabhängig von fremden Fenstern auf Deutsch und Englisch", async () => {
    const user = userEvent.setup();
    const preset = presetFixture();
    vi.spyOn(worksetsApi, "planPresetApply").mockResolvedValue({
      presetId: preset.id,
      canApply: true,
      confirmationReason: "experimentalDesktopProvider",
      desktopChanges: 1,
      reusedWindows: 1,
      launchedWindows: 0,
      closedWindows: 0,
      adjustedWindows: 0,
      risks: [{
        id: "experimental",
        code: "experimentalDesktopProvider",
        kind: "desktopExperimental",
        title: "Experimental desktop control",
        detail: "Virtual desktop mutations require confirmation for this session.",
        blocking: true,
        affectedItemIds: [],
      }],
    });
    vi.spyOn(worksetsApi, "applyPreset").mockResolvedValue(reportFixture());

    render(<ApplyFlow preset={preset} onClose={vi.fn()} onCompleted={vi.fn()} onDiagnostics={vi.fn()} />);

    const start = await screen.findByRole("button", { name: "Jetzt wechseln" });
    const germanLabel = "Experimentelle virtuelle Desktop-Steuerung für diese Sitzung erlauben";
    expect(screen.getByRole("checkbox", { name: germanLabel })).not.toBeChecked();
    expect(screen.queryByRole("checkbox", { name: /Fremde Fenster/ })).not.toBeInTheDocument();
    expect(start).toBeDisabled();
    await expectAccessible();

    await act(async () => {
      await i18n.changeLanguage("en");
    });

    const englishLabel = "Allow experimental virtual desktop control for this session";
    const confirmation = screen.getByRole("checkbox", { name: englishLabel });
    expect(screen.getByRole("button", { name: "Switch now" })).toBeDisabled();
    await user.click(confirmation);
    const englishStart = screen.getByRole("button", { name: "Switch now" });
    expect(englishStart).toBeEnabled();
    await user.click(englishStart);

    expect(await screen.findByRole("heading", { name: "Your workspace is ready" })).toBeVisible();
    expect(worksetsApi.applyPreset).toHaveBeenCalledWith(preset.id, { confirmDesktopMutation: true });
  });
});
