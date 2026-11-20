import {
  Button,
  FluentProvider,
  MessageBar,
  MessageBarBody,
  Spinner,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add20Regular,
  Apps24Regular,
  DesktopEdit24Regular,
  Globe20Regular,
  Navigation24Regular,
  Pulse24Regular,
  Settings24Regular,
} from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { createNewPresetDraft, isTauriRuntime, worksetsApi, worksetsEvents } from "./api";
import "./App.css";
import { ApplyFlow } from "./components/ApplyFlow";
import { WorkSetsWordmark } from "./components/Brand";
import { CaptureDialog } from "./components/CaptureDialog";
import { Dashboard } from "./components/Dashboard";
import { DiagnosticsView } from "./components/DiagnosticsView";
import { ImportDialog } from "./components/ImportDialog";
import { Onboarding } from "./components/Onboarding";
import { PresetEditor } from "./components/PresetEditor";
import { SettingsView } from "./components/SettingsView";
import { normalizeShortcut } from "./components/ShortcutInput";
import { worksetsDarkTheme, worksetsLightTheme } from "./theme";
import type { AppSettings, AppState, ApplyReport, PresetV1, UpdateInfo, ViewId } from "./types";

function useSystemDark() {
  const [dark, setDark] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false);
  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const update = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return dark;
}

function App() {
  const { t, i18n } = useTranslation();
  const systemDark = useSystemDark();
  const [state, setState] = useState<AppState>();
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [view, setView] = useState<ViewId>("presets");
  const [captureOpen, setCaptureOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [editingPreset, setEditingPreset] = useState<PresetV1>();
  const [applyTarget, setApplyTarget] = useState<PresetV1>();
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo>();
  const [notice, setNotice] = useState("");
  const [mobileNav, setMobileNav] = useState(false);
  const [creatingPreset, setCreatingPreset] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError("");
    try {
      const appState = await worksetsApi.getAppState();
      setState(appState);
      await i18n.changeLanguage(appState.settings.language);
      if (appState.settings.onboardingComplete && appState.settings.checkForUpdates) {
        void worksetsApi.checkForUpdates().then(setUpdateInfo).catch(() => undefined);
      }
    } catch (reason) {
      setLoadError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [i18n]);

  useEffect(() => { void load(); }, [load]);

  useEffect(() => {
    document.documentElement.lang = state?.settings.language ?? "de";
  }, [state?.settings.language]);

  useEffect(() => {
    if (!state || !isTauriRuntime()) return;
    let active = true;
    const cleanups: Array<() => void> = [];
    const addCleanup = (cleanup: () => void) => {
      if (active) cleanups.push(cleanup);
      else cleanup();
    };
    const selectPreset = (presetId: string) => {
      const preset = state.presets.find((candidate) => candidate.id === presetId);
      if (preset) {
        setView("presets");
        setApplyTarget(preset);
      }
    };

    void worksetsEvents.onTrayApply(selectPreset).then(addCleanup).catch(() => undefined);
    void worksetsEvents.onGlobalShortcut((shortcut) => {
      const normalized = normalizeShortcut(shortcut);
      if (normalized === normalizeShortcut(state.settings.captureHotkey)) {
        setView("presets");
        setCaptureOpen(true);
        return;
      }
      const preset = state.presets.find((candidate) => candidate.hotkey && normalizeShortcut(candidate.hotkey) === normalized);
      if (preset) selectPreset(preset.id);
      else setNotice(t("app.shortcutUnavailable"));
    }).then(addCleanup).catch(() => undefined);

    return () => {
      active = false;
      for (const cleanup of cleanups) cleanup();
    };
  }, [state, t]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 3000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const saveSettings = async (settings: AppSettings) => {
    const saved = await worksetsApi.saveSettings(settings);
    setState((current) => current ? { ...current, settings: saved } : current);
    await i18n.changeLanguage(saved.language);
  };

  const finishOnboarding = async (settings: AppSettings, capture: boolean) => {
    await saveSettings(settings);
    if (capture) setCaptureOpen(true);
  };

  const createPreset = async () => {
    if (creatingPreset) return;
    setCreatingPreset(true);
    try {
      const draft = await createNewPresetDraft("");
      if (state) draft.desktopPolicy = state.settings.defaultPolicy;
      setEditingPreset(draft);
      setView("editor");
      setMobileNav(false);
    } catch (reason) {
      setNotice(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCreatingPreset(false);
    }
  };

  const selectView = (nextView: ViewId) => {
    if (nextView === "editor" && !editingPreset) {
      void createPreset();
      return;
    }
    setView(nextView);
    setMobileNav(false);
    if (nextView !== "editor") setEditingPreset(undefined);
  };

  const openEditor = (preset: PresetV1) => {
    setEditingPreset(structuredClone(preset));
    setView("editor");
    setCaptureOpen(false);
  };

  const savePreset = async (preset: PresetV1) => {
    const saved = await worksetsApi.savePreset(preset);
    setState((current) => current ? {
      ...current,
      presets: [saved, ...current.presets.filter((entry) => entry.id !== saved.id)],
    } : current);
    setEditingPreset(undefined);
    setView("presets");
    setNotice(t("editor.saved"));
  };

  const duplicatePreset = async (preset: PresetV1) => {
    const now = new Date().toISOString();
    const copy: PresetV1 = {
      ...structuredClone(preset),
      id: crypto.randomUUID(),
      name: t("dashboard.duplicateName", { name: preset.name }),
      createdAt: now,
      updatedAt: now,
      hotkey: undefined,
    };
    const saved = await worksetsApi.savePreset(copy);
    setState((current) => current ? { ...current, presets: [saved, ...current.presets] } : current);
    setNotice(t("common.duplicate"));
  };

  const deletePreset = async (preset: PresetV1) => {
    await worksetsApi.deletePreset(preset.id);
    setState((current) => current ? { ...current, presets: current.presets.filter((entry) => entry.id !== preset.id) } : current);
  };

  const exportPreset = async (preset: PresetV1) => {
    const json = await worksetsApi.exportPreset(preset.id);
    await navigator.clipboard.writeText(json);
    setNotice(t("importExport.exported"));
  };

  const updateApplyReport = (report: ApplyReport) => {
    setState((current) => current ? { ...current, lastApplyReport: report } : current);
  };

  const openExternal = async (url: string) => {
    if (isTauriRuntime()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  };

  const quickLanguageToggle = async () => {
    if (!state) return;
    const settings = { ...state.settings, language: state.settings.language === "de" ? "en" as const : "de" as const };
    await saveSettings(settings);
  };

  const isDark = state?.settings.theme === "dark" || (state?.settings.theme === "system" && systemDark);

  if (loading || !state) {
    return (
      <FluentProvider theme={isDark ? worksetsDarkTheme : worksetsLightTheme} className="app-provider">
        <main className="boot-screen">
          <WorkSetsWordmark />
          {loadError ? (
            <><MessageBar intent="error"><MessageBarBody>{loadError}</MessageBarBody></MessageBar><Button appearance="primary" onClick={() => void load()}>{t("common.retry")}</Button></>
          ) : <Spinner size="large" label="WorkSets" />}
        </main>
      </FluentProvider>
    );
  }

  if (!state.settings.onboardingComplete) {
    return (
      <FluentProvider theme={isDark ? worksetsDarkTheme : worksetsLightTheme} className="app-provider">
        <div className="language-float"><Button appearance="subtle" icon={<Globe20Regular />} aria-label={t("app.switchLanguage")} onClick={() => void quickLanguageToggle()}>{state.settings.language === "de" ? "EN" : "DE"}</Button></div>
        <Onboarding compatibility={state.compatibility} settings={state.settings} onFinish={finishOnboarding} />
        <CaptureDialog open={captureOpen} onClose={() => setCaptureOpen(false)} onContinue={openEditor} />
      </FluentProvider>
    );
  }

  return (
    <FluentProvider theme={isDark ? worksetsDarkTheme : worksetsLightTheme} className="app-provider">
      <div className={`app-shell${mobileNav ? " mobile-nav-open" : ""}`}>
        <aside className="sidebar">
          <div className="sidebar__top">
            <WorkSetsWordmark />
            <span className="alpha-pill">{t("app.alpha")}</span>
          </div>
          <nav aria-label={t("nav.main")}>
            <Button appearance="subtle" icon={<Apps24Regular />} className={view === "presets" ? "is-active" : ""} aria-current={view === "presets" ? "page" : undefined} onClick={() => selectView("presets")}>{t("nav.presets")}</Button>
            <Button appearance="subtle" icon={<DesktopEdit24Regular />} className={view === "editor" ? "is-active" : ""} aria-current={view === "editor" ? "page" : undefined} onClick={() => selectView("editor")}>{t("nav.editor")}</Button>
            <Button appearance="subtle" icon={<Settings24Regular />} className={view === "settings" ? "is-active" : ""} aria-current={view === "settings" ? "page" : undefined} onClick={() => selectView("settings")}>{t("nav.settings")}</Button>
            <Button appearance="subtle" icon={<Pulse24Regular />} className={view === "diagnostics" ? "is-active" : ""} aria-current={view === "diagnostics" ? "page" : undefined} onClick={() => selectView("diagnostics")}>{t("nav.diagnostics")}</Button>
          </nav>
          <div className="sidebar__footer">
            {!isTauriRuntime() && <Tooltip content={t("app.browserModeDetail")} relationship="description"><span className="runtime-badge"><i />{t("app.browserMode")}</span></Tooltip>}
            <Button appearance="subtle" icon={<Globe20Regular />} aria-label={t("app.switchLanguage")} onClick={() => void quickLanguageToggle()}>{state.settings.language === "de" ? "English" : "Deutsch"}</Button>
            <span>0.1.0-alpha.1</span>
          </div>
        </aside>

        <header className="mobile-header">
          <Button appearance="subtle" icon={<Navigation24Regular />} aria-label={t("common.menu")} onClick={() => setMobileNav((open) => !open)} />
          <WorkSetsWordmark compact />
          <Button appearance="subtle" icon={<Add20Regular />} aria-label={t("dashboard.newPreset")} disabled={creatingPreset} onClick={() => void createPreset()} />
        </header>
        {mobileNav && <button type="button" className="nav-backdrop" aria-label={t("common.close")} onClick={() => setMobileNav(false)} />}

        <main className="app-content">
          {notice && <div className="app-notice" role="status">{notice}</div>}
          {(applyTarget || creatingPreset) && <div className="busy-indicator"><Spinner size="tiny" /><span>{creatingPreset ? t("app.preparingPreset") : t("apply.planning")}</span></div>}
          {view === "presets" && (
            <Dashboard
              presets={state.presets}
              lastApplyReport={state.lastApplyReport}
              updateInfo={updateInfo}
              onCreate={() => void createPreset()}
              onCapture={() => setCaptureOpen(true)}
              onEdit={openEditor}
              onDuplicate={duplicatePreset}
              onDelete={deletePreset}
              onApply={setApplyTarget}
              onExport={exportPreset}
              onImport={() => setImportOpen(true)}
              onOpenUpdate={(url) => void openExternal(url)}
              onDismissUpdate={() => setUpdateInfo(undefined)}
            />
          )}
          {view === "editor" && editingPreset && (
            <PresetEditor
              key={editingPreset.id}
              initialPreset={editingPreset}
              onSave={savePreset}
              onCancel={() => selectView("presets")}
              reservedHotkeys={[
                state.settings.captureHotkey,
                ...state.presets.filter((preset) => preset.id !== editingPreset.id).flatMap((preset) => preset.hotkey ? [preset.hotkey] : []),
              ]}
            />
          )}
          {view === "settings" && <SettingsView settings={state.settings} presets={state.presets} onSave={saveSettings} onOpenUpdate={(url) => void openExternal(url)} />}
          {view === "diagnostics" && <DiagnosticsView />}
        </main>
      </div>

      <CaptureDialog open={captureOpen} onClose={() => setCaptureOpen(false)} onContinue={openEditor} />
      <ImportDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImported={(preset) => {
          setState((current) => current ? { ...current, presets: [preset, ...current.presets] } : current);
          setImportOpen(false);
          setNotice(t("importExport.valid"));
        }}
      />
      <ApplyFlow
        preset={applyTarget}
        onClose={() => setApplyTarget(undefined)}
        onCompleted={updateApplyReport}
        onDiagnostics={() => selectView("diagnostics")}
      />
    </FluentProvider>
  );
}

export default App;
