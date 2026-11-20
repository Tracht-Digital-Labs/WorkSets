import {
  Button,
  Divider,
  Field,
  MessageBar,
  MessageBarBody,
  Radio,
  RadioGroup,
  Select,
  Switch,
} from "@fluentui/react-components";
import { ArrowClockwise20Regular, Open20Regular, Save20Regular } from "@fluentui/react-icons";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { worksetsApi } from "../api";
import type { AppSettings, DesktopPolicy, Language, PresetV1, ThemePreference, UpdateInfo } from "../types";
import { WorkSetsMark } from "./Brand";
import { ShortcutInput } from "./ShortcutInput";

interface SettingsViewProps {
  settings: AppSettings;
  presets: PresetV1[];
  onSave: (settings: AppSettings) => Promise<void>;
  onOpenUpdate: (url: string) => void;
}

export function SettingsView({ settings, presets, onSave, onOpenUpdate }: SettingsViewProps) {
  const { t, i18n } = useTranslation();
  const [draft, setDraft] = useState(settings);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo>();
  const releaseUrl = update?.releaseUrl;

  useEffect(() => setDraft(settings), [settings]);
  const hotkeyConflict = useMemo(
    () => Boolean(draft.captureHotkey && presets.some((preset) => preset.hotkey?.toLowerCase() === draft.captureHotkey.toLowerCase())),
    [draft.captureHotkey, presets],
  );

  const save = async () => {
    if (hotkeyConflict) return;
    setSaving(true);
    setSaved(false);
    try {
      await onSave(draft);
      setSaved(true);
    } finally {
      setSaving(false);
    }
  };

  const checkUpdate = async () => {
    setChecking(true);
    try {
      setUpdate(await worksetsApi.checkForUpdates());
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="view settings-view">
      <header className="view-header">
        <div><h1>{t("settings.title")}</h1><p>{t("settings.subtitle")}</p></div>
        <Button appearance="primary" icon={<Save20Regular />} disabled={saving || hotkeyConflict} onClick={() => void save()}>
          {saving ? t("common.saving") : t("common.save")}
        </Button>
      </header>
      {saved && <MessageBar intent="success"><MessageBarBody>{t("settings.saved")}</MessageBarBody></MessageBar>}

      <div className="settings-columns">
        <div className="settings-column">
          <section className="settings-card">
            <h2>{t("settings.appearance")}</h2>
            <Field label={t("settings.language")}>
              <Select
                value={draft.language}
                onChange={(event) => {
                  const language = event.target.value as Language;
                  setDraft((current) => ({ ...current, language }));
                  void i18n.changeLanguage(language);
                }}
              >
                <option value="de">Deutsch</option>
                <option value="en">English</option>
              </Select>
            </Field>
            <Field label={t("settings.theme")}>
              <RadioGroup
                layout="horizontal"
                value={draft.theme}
                onChange={(_, data) => setDraft((current) => ({ ...current, theme: data.value as ThemePreference }))}
              >
                <Radio value="system" label={t("settings.system")} />
                <Radio value="light" label={t("settings.light")} />
                <Radio value="dark" label={t("settings.dark")} />
              </RadioGroup>
            </Field>
          </section>

          <section className="settings-card">
            <h2>{t("settings.behavior")}</h2>
            <div className="settings-switches">
              <Switch checked={draft.runAtStartup} label={t("settings.startup")} onChange={(_, data) => setDraft((current) => ({ ...current, runAtStartup: data.checked }))} />
              <Switch checked={draft.minimizeToTray} label={t("settings.tray")} onChange={(_, data) => setDraft((current) => ({ ...current, minimizeToTray: data.checked }))} />
              <Switch checked={draft.checkForUpdates} label={t("settings.updates")} onChange={(_, data) => setDraft((current) => ({ ...current, checkForUpdates: data.checked }))} />
            </div>
            <Field label={t("settings.defaultPolicy")}>
              <Select value={draft.defaultPolicy} onChange={(event) => setDraft((current) => ({ ...current, defaultPolicy: event.target.value as DesktopPolicy }))}>
                <option value="exactProtected">{t("editor.exactProtected")}</option>
                <option value="managedOnly">{t("editor.managedOnly")}</option>
              </Select>
            </Field>
            <Field
              label={t("settings.captureHotkey")}
              validationState={hotkeyConflict ? "error" : "none"}
              validationMessage={hotkeyConflict ? t("settings.hotkeyConflict") : undefined}
            >
              <ShortcutInput value={draft.captureHotkey} onChange={(captureHotkey) => setDraft((current) => ({ ...current, captureHotkey }))} />
            </Field>
          </section>
        </div>

        <div className="settings-column">
          <section className="settings-card">
            <h2>{t("settings.privacy")}</h2>
            <Switch checked={draft.telemetryOptIn} label={t("settings.telemetry")} onChange={(_, data) => setDraft((current) => ({ ...current, telemetryOptIn: data.checked }))} />
            <p className="settings-hint">{t("settings.telemetryHint")}</p>
          </section>

          <section className="settings-card update-card">
            <div className="settings-card__heading"><h2>{t("settings.updates")}</h2><Button appearance="secondary" icon={<ArrowClockwise20Regular />} disabled={checking} onClick={() => void checkUpdate()}>{checking ? t("settings.checking") : t("settings.checkNow")}</Button></div>
            {update && (
              update.available && update.latestVersion && releaseUrl ? (
                <MessageBar intent="info">
                  <MessageBarBody>
                    <strong>{t("update.title", { version: update.latestVersion })}</strong>
                    <Button appearance="transparent" icon={<Open20Regular />} onClick={() => onOpenUpdate(releaseUrl)}>{t("update.openRelease")}</Button>
                  </MessageBarBody>
                </MessageBar>
              ) : update.error ? (
                <MessageBar intent="warning"><MessageBarBody>{t("update.unavailable")}</MessageBarBody></MessageBar>
              ) : <MessageBar intent="success"><MessageBarBody>{t("update.upToDate")}</MessageBarBody></MessageBar>
            )}
          </section>

          <section className="settings-card about-card">
            <div className="about-brand"><WorkSetsMark size={54} decorative /><div><h2>WorkSets</h2><span>{t("app.byline")}</span></div></div>
            <Divider />
            <p>{t("settings.version")}</p>
            <p>{t("settings.license")}</p>
          </section>
        </div>
      </div>
    </div>
  );
}
