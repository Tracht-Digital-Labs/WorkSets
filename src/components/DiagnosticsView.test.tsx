import { render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { worksetsApi } from "../api";
import { expectAccessible } from "../test/accessibility";
import { DiagnosticsView } from "./DiagnosticsView";

describe("DiagnosticsView", () => {
  it("lokalisiert feste Prüftexte und erhält dynamische Brokerdetails", async () => {
    vi.spyOn(worksetsApi, "getDiagnostics").mockResolvedValue({
      appVersion: "0.1.0-alpha.1",
      generatedAt: "2026-08-21T08:00:00.000Z",
      storagePath: "C:\\WorkSets",
      brokerProtocol: "v1 · windows11_25h2",
      sessionMutationsEnabled: true,
      checks: [
        {
          id: "storage",
          label: "Preset storage",
          status: "ok",
          detail: "2 presets available.",
          detailKey: "available",
          detailValues: { count: 2 },
        },
        {
          id: "broker",
          label: "Desktop broker",
          status: "warning",
          detail: "winvd-0.0.49+build-26200",
        },
        {
          id: "privacy",
          label: "Telemetry privacy",
          status: "ok",
          detail: "No sensitive fields or transport enabled.",
          detailKey: "safe",
        },
      ],
    });

    render(<DiagnosticsView />);

    expect(await screen.findByText("Preset-Speicher")).toBeVisible();
    expect(screen.getByText("2 Presets sind verfügbar.")).toBeVisible();
    expect(screen.getByText("Telemetrie-Datenschutz")).toBeVisible();
    expect(screen.getByText("Es sind weder sensible Felder noch eine Übertragung aktiviert.")).toBeVisible();
    expect(screen.getByText("winvd-0.0.49+build-26200")).toBeVisible();
    expect(screen.queryByText("Preset storage")).not.toBeInTheDocument();
    expect(screen.getByText("Warnung")).toBeVisible();
    await expectAccessible();
  });
});
