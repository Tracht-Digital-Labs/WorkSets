import {
  Button,
  Field,
  MessageBar,
  MessageBarBody,
  MessageBarTitle,
  ProgressBar,
  Spinner,
  Switch,
} from "@fluentui/react-components";
import { CheckmarkCircle24Filled, LockClosed20Regular, Warning24Filled } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppSettings, CompatibilityInfo } from "../types";
import { ShortcutInput } from "./ShortcutInput";
import { WorkSetsMark, WorkSetsWordmark } from "./Brand";

interface OnboardingProps {
  compatibility?: CompatibilityInfo;
  settings: AppSettings;
  onFinish: (settings: AppSettings, capture: boolean) => Promise<void>;
}

export function Onboarding({ compatibility, settings, onFinish }: OnboardingProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState(settings);
  const [finishing, setFinishing] = useState(false);
  const steps = [t("onboarding.stepCompatibility"), t("onboarding.stepConvenience"), t("onboarding.stepReady")];

  const finish = async (capture: boolean) => {
    setFinishing(true);
    try {
      await onFinish({ ...draft, onboardingComplete: true }, capture);
    } finally {
      setFinishing(false);
    }
  };

  return (
    <main className="onboarding-shell">
      <header className="onboarding-topbar">
        <WorkSetsWordmark />
        <span className="alpha-pill">{t("app.alpha")}</span>
      </header>

      <div className="onboarding-layout">
        <aside className="onboarding-intro" aria-labelledby="onboarding-title">
          <div className="onboarding-mark-wrap">
            <WorkSetsMark size={72} decorative />
          </div>
          <p className="eyebrow">{t("onboarding.eyebrow")}</p>
          <h1 id="onboarding-title">{t("onboarding.title")}</h1>
          <p className="lead">{t("onboarding.intro")}</p>
          <div className="privacy-note">
            <LockClosed20Regular aria-hidden />
            <span>{t("onboarding.privacy")}</span>
          </div>
        </aside>

        <section className="onboarding-panel" aria-live="polite">
          <ol className="stepper" aria-label={`${step + 1} / ${steps.length}`}>
            {steps.map((label, index) => (
              <li key={label} className={index === step ? "is-current" : index < step ? "is-complete" : ""}>
                <span>{index < step ? <CheckmarkCircle24Filled aria-hidden /> : index + 1}</span>
                <small className="stepper__label">{label}</small>
              </li>
            ))}
          </ol>
          <ProgressBar value={(step + 1) / steps.length} thickness="medium" aria-label={`${step + 1} / ${steps.length}`} />

          {step === 0 && (
            <div className="onboarding-step">
              <div>
                <p className="eyebrow">01</p>
                <h2>{t("onboarding.compatibilityTitle")}</h2>
                <p>{t("onboarding.compatibilityBody")}</p>
              </div>
              {!compatibility ? (
                <div className="system-check-card">
                  <Spinner size="small" />
                  <span>{t("onboarding.checking")}</span>
                </div>
              ) : (
                <div className={`system-check-card system-check-card--${compatibility.desktopCapability}`}>
                  {compatibility.desktopCapability === "available" ? (
                    <CheckmarkCircle24Filled aria-hidden />
                  ) : (
                    <Warning24Filled aria-hidden />
                  )}
                  <div>
                    <strong>
                      {compatibility.desktopCapability === "available"
                        ? t("onboarding.supported")
                        : compatibility.desktopCapability === "experimental"
                          ? t("onboarding.experimental")
                          : t("onboarding.unsupported")}
                    </strong>
                    <span>
                      {compatibility.platform} · {compatibility.architecture}
                      {compatibility.windowsBuild ? ` · Build ${compatibility.windowsBuild}` : ""}
                    </span>
                  </div>
                </div>
              )}
              {compatibility?.desktopCapability === "unavailable" && (
                <MessageBar intent="warning">
                  <MessageBarBody>
                    <MessageBarTitle>{t("onboarding.continueLimited")}</MessageBarTitle>
                  </MessageBarBody>
                </MessageBar>
              )}
              <div className="dialog-footer">
                <Button appearance="primary" disabled={!compatibility} onClick={() => setStep(1)}>
                  {t("common.continue")}
                </Button>
              </div>
            </div>
          )}

          {step === 1 && (
            <div className="onboarding-step">
              <div>
                <p className="eyebrow">02</p>
                <h2>{t("onboarding.convenienceTitle")}</h2>
                <p>{t("onboarding.convenienceBody")}</p>
              </div>
              <div className="settings-list">
                <Switch
                  checked={draft.runAtStartup}
                  onChange={(_, data) => setDraft((current) => ({ ...current, runAtStartup: data.checked }))}
                  label={t("onboarding.startup")}
                />
                <p>{t("onboarding.startupHint")}</p>
                <Switch
                  checked={draft.minimizeToTray}
                  onChange={(_, data) => setDraft((current) => ({ ...current, minimizeToTray: data.checked }))}
                  label={t("onboarding.tray")}
                />
                <p>{t("onboarding.trayHint")}</p>
                <Field label={t("onboarding.hotkey")} hint={t("onboarding.hotkeyHint")}>
                  <ShortcutInput
                    aria-label={t("onboarding.hotkey")}
                    value={draft.captureHotkey}
                    onChange={(captureHotkey) => setDraft((current) => ({ ...current, captureHotkey }))}
                  />
                </Field>
              </div>
              <div className="dialog-footer dialog-footer--split">
                <Button appearance="subtle" onClick={() => setStep(0)}>
                  {t("common.back")}
                </Button>
                <Button appearance="primary" onClick={() => setStep(2)}>
                  {t("common.continue")}
                </Button>
              </div>
            </div>
          )}

          {step === 2 && (
            <div className="onboarding-step onboarding-step--ready">
              <div className="ready-illustration" aria-hidden="true">
                <span className="ready-monitor"><span /></span>
                <span className="ready-monitor ready-monitor--small"><span /></span>
                <WorkSetsMark size={54} decorative />
              </div>
              <div>
                <p className="eyebrow">03</p>
                <h2>{t("onboarding.readyTitle")}</h2>
                <p>{t("onboarding.readyBody")}</p>
              </div>
              <div className="onboarding-actions">
                <Button appearance="primary" size="large" disabled={finishing} onClick={() => void finish(true)}>
                  {finishing ? t("common.saving") : t("onboarding.captureNow")}
                </Button>
                <Button appearance="subtle" disabled={finishing} onClick={() => void finish(false)}>
                  {t("onboarding.skip")}
                </Button>
              </div>
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
