import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  ProgressBar,
  Spinner,
} from "@fluentui/react-components";
import {
  ArrowSync20Regular,
  CheckmarkCircle24Filled,
  DismissCircle24Filled,
  ErrorCircle20Regular,
  Info20Regular,
  Warning20Regular,
} from "@fluentui/react-icons";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauriRuntime, worksetsApi, worksetsEvents } from "../api";
import type { ApplyOptions, ApplyPlan, ApplyReport, PresetV1 } from "../types";

interface ApplyFlowProps {
  preset?: PresetV1;
  onClose: () => void;
  onCompleted: (report: ApplyReport) => void;
  onDiagnostics: () => void;
}

const stepIds = ["preflight", "desktops", "reuse", "launch", "verify", "close", "finalize"];

function needsExplicitReview(plan: ApplyPlan) {
  return !plan.canApply || plan.risks.some((risk) => risk.blocking);
}

export function ApplyFlow({ preset, onClose, onCompleted, onDiagnostics }: ApplyFlowProps) {
  const { t } = useTranslation();
  const [stage, setStage] = useState<"planning" | "review" | "progress" | "result">("planning");
  const [plan, setPlan] = useState<ApplyPlan>();
  const [report, setReport] = useState<ApplyReport>();
  const [error, setError] = useState("");
  const [progressIndex, setProgressIndex] = useState(0);
  const [options, setOptions] = useState<ApplyOptions>({});
  const [confirmForceClose, setConfirmForceClose] = useState(false);
  const requestToken = useRef(0);

  const runApply = useCallback(async (target: PresetV1, applyOptions: ApplyOptions, token: number) => {
    setStage("progress");
    setProgressIndex(0);
    setError("");
    setConfirmForceClose(false);
    let stopProgress: () => void = () => undefined;
    try {
      try {
        stopProgress = await worksetsEvents.onApplyProgress((progress) => {
          if (progress.presetId !== target.id) return;
          const index = stepIds.indexOf(progress.step);
          if (index >= 0) setProgressIndex(index);
        });
      } catch {
        // Applying remains safe when the optional progress channel is unavailable.
      }
      const nextReport = await worksetsApi.applyPreset(target.id, applyOptions);
      if (requestToken.current !== token) return;
      setProgressIndex(stepIds.length - 1);
      setReport(nextReport);
      setStage("result");
      onCompleted(nextReport);
    } catch (reason) {
      if (requestToken.current !== token) return;
      const failedReport: ApplyReport = {
        id: crypto.randomUUID(),
        presetId: target.id,
        status: "failed",
        startedAt: new Date().toISOString(),
        finishedAt: new Date().toISOString(),
        steps: [],
        issues: [{ id: crypto.randomUUID(), severity: "error", title: t("apply.failedIssueTitle"), detail: reason instanceof Error ? reason.message : String(reason) }],
        reusedWindows: 0,
        launchedWindows: 0,
        closedWindows: 0,
        adjustedWindows: 0,
      };
      setReport(failedReport);
      setStage("result");
      onCompleted(failedReport);
    } finally {
      stopProgress();
    }
  }, [onCompleted, t]);

  useEffect(() => {
    if (!preset) return;
    const token = ++requestToken.current;
    setStage("planning");
    setPlan(undefined);
    setReport(undefined);
    setError("");
    setProgressIndex(0);
    setOptions({});
    setConfirmForceClose(false);
    void worksetsApi.planPresetApply(preset.id).then((nextPlan) => {
      if (requestToken.current !== token) return;
      setPlan(nextPlan);
      if (needsExplicitReview(nextPlan)) setStage("review");
      else void runApply(preset, {}, token);
    }).catch((reason) => {
      if (requestToken.current !== token) return;
      setError(reason instanceof Error ? reason.message : String(reason));
      setStage("review");
    });
  }, [preset, runApply]);

  useEffect(() => {
    if (stage !== "progress" || isTauriRuntime()) return;
    const timer = window.setInterval(() => setProgressIndex((current) => Math.min(stepIds.length - 1, current + 1)), 260);
    return () => window.clearInterval(timer);
  }, [stage]);

  const start = () => {
    if (!preset) return;
    const token = ++requestToken.current;
    void runApply(preset, options, token);
  };

  const retryWithForce = () => {
    if (!preset) return;
    const token = ++requestToken.current;
    void runApply(preset, { ...options, forceOwnedProcesses: true }, token);
  };

  const cancel = async () => {
    requestToken.current += 1;
    await worksetsApi.cancelApply();
    if (!preset) return;
    const cancelled: ApplyReport = {
      id: crypto.randomUUID(),
      presetId: preset.id,
      status: "cancelled",
      startedAt: new Date().toISOString(),
      finishedAt: new Date().toISOString(),
      steps: [],
      issues: [],
      reusedWindows: 0,
      launchedWindows: 0,
      closedWindows: 0,
      adjustedWindows: 0,
    };
    setReport(cancelled);
    setStage("result");
    onCompleted(cancelled);
  };

  const close = () => {
    requestToken.current += 1;
    onClose();
  };

  const foreignRisk = plan?.risks.some((risk) => risk.kind === "foreignWindows" && risk.blocking);
  const experimentalDesktopRisk = plan?.risks.some((risk) => risk.kind === "desktopExperimental" && risk.blocking);
  const closeBlocked = report?.issues.some((issue) => issue.id === "closeBlocked") ?? false;
  const canStart = Boolean(plan?.canApply)
    && (!foreignRisk || options.confirmForeignWindows)
    && (!experimentalDesktopRisk || options.confirmDesktopMutation);
  const resultTitle = report?.status === "success"
    ? t("apply.successTitle")
    : report?.status === "partial"
      ? t("apply.partialTitle")
      : report?.status === "cancelled"
        ? t("apply.cancelledTitle")
        : t("apply.failedTitle");

  return (
    <Dialog open={Boolean(preset)} onOpenChange={(_, data) => !data.open && stage !== "progress" && close()}>
      <DialogSurface className="apply-dialog">
        <DialogBody>
          {stage === "planning" && (
            <>
              <DialogTitle>{t("apply.planning")}</DialogTitle>
              <DialogContent className="apply-loading">
                <Spinner size="large" label={t("apply.planning")} />
                <Button appearance="secondary" onClick={close}>{t("common.cancel")}</Button>
              </DialogContent>
            </>
          )}

          {stage === "review" && (
            <>
              <DialogTitle>{t("apply.title", { name: preset?.name })}</DialogTitle>
              <DialogContent>
                {error ? (
                  <MessageBar intent="error"><MessageBarBody>{error}</MessageBarBody></MessageBar>
                ) : plan ? (
                  <div className="apply-review">
                    <p className="apply-summary">{t("apply.summary", { reuse: plan.reusedWindows, launch: plan.launchedWindows, close: plan.closedWindows })}</p>
                    <section className="apply-metrics" aria-label={t("common.status")}>
                      <span><strong>{plan.desktopChanges}</strong><small>{t("nav.presets")}</small></span>
                      <span><strong>{plan.reusedWindows}</strong><small>{t("inspector.reuse")}</small></span>
                      <span><strong>{plan.launchedWindows}</strong><small>{t("inspector.restart")}</small></span>
                      <span><strong>{plan.adjustedWindows}</strong><small>{t("dashboard.attention")}</small></span>
                    </section>
                    <h3>{t("apply.risksTitle")}</h3>
                    {plan.risks.length === 0 ? (
                      <MessageBar intent="success"><MessageBarBody>{t("apply.noRisks")}</MessageBarBody></MessageBar>
                    ) : (
                      <div className="risk-list">
                        {plan.risks.map((risk) => (
                          <article key={risk.id} className={`risk-item${risk.blocking ? " risk-item--blocking" : ""}`}>
                            {risk.blocking ? <Warning20Regular aria-hidden /> : <Info20Regular aria-hidden />}
                            <div>
                              <strong>{t(`apply.${risk.kind}`)}</strong>
                              <span>{t(`apply.notices.${risk.code}.title`, { count: risk.count, defaultValue: risk.title })}</span>
                              <small>{t(`apply.notices.${risk.code}.detail`, { count: risk.count, defaultValue: risk.detail })}</small>
                            </div>
                            {risk.blocking && <Badge appearance="tint" color="warning">{t("dashboard.attention")}</Badge>}
                          </article>
                        ))}
                      </div>
                    )}
                    {foreignRisk && (
                      <Checkbox
                        checked={Boolean(options.confirmForeignWindows)}
                        label={t("apply.confirmForeign")}
                        aria-label={t("apply.confirmForeign")}
                        onChange={(_, data) => setOptions((current) => ({ ...current, confirmForeignWindows: Boolean(data.checked) }))}
                      />
                    )}
                    {experimentalDesktopRisk && (
                      <Checkbox
                        checked={Boolean(options.confirmDesktopMutation)}
                        label={t("apply.confirmExperimentalDesktop")}
                        aria-label={t("apply.confirmExperimentalDesktop")}
                        onChange={(_, data) => setOptions((current) => ({ ...current, confirmDesktopMutation: Boolean(data.checked) }))}
                      />
                    )}
                  </div>
                ) : null}
              </DialogContent>
              <DialogActions>
                <Button appearance="secondary" onClick={close}>{t("common.cancel")}</Button>
                {error ? (
                  <Button appearance="primary" onClick={() => preset && worksetsApi.planPresetApply(preset.id).then((next) => { setPlan(next); setError(""); })}>{t("common.retry")}</Button>
                ) : (
                  <Button appearance="primary" icon={<ArrowSync20Regular />} disabled={!canStart} onClick={() => void start()}>{t("apply.start")}</Button>
                )}
              </DialogActions>
            </>
          )}

          {stage === "progress" && (
            <DialogContent className="apply-progress">
              <div className="progress-orbit" aria-hidden="true"><ArrowSync20Regular /></div>
              <h2>{t("apply.progressTitle")}</h2>
              <p>{t("apply.progressBody")}</p>
              <ProgressBar value={(progressIndex + 0.4) / stepIds.length} thickness="large" />
              <ol className="apply-steps" aria-live="polite">
                {stepIds.map((step, index) => (
                  <li key={step} className={index < progressIndex ? "is-complete" : index === progressIndex ? "is-current" : ""}>
                    <span>{index < progressIndex ? <CheckmarkCircle24Filled aria-hidden /> : index + 1}</span>
                    {t(`apply.step${step[0].toUpperCase()}${step.slice(1)}`)}
                  </li>
                ))}
              </ol>
              <Button appearance="subtle" onClick={() => void cancel()}>{t("apply.cancel")}</Button>
            </DialogContent>
          )}

          {stage === "result" && report && (
            <>
              <DialogContent className="apply-result">
                <div className={`result-icon result-icon--${report.status}`}>
                  {report.status === "success" ? <CheckmarkCircle24Filled aria-hidden /> : report.status === "partial" ? <Warning20Regular aria-hidden /> : report.status === "cancelled" ? <ErrorCircle20Regular aria-hidden /> : <DismissCircle24Filled aria-hidden />}
                </div>
                <h2>{resultTitle}</h2>
                <p>{t("apply.reportSummary", { launched: report.launchedWindows, reused: report.reusedWindows, adjusted: report.adjustedWindows })}</p>
                {report.issues.length > 0 && (
                  <section className="result-issues">
                    <h3>{t("apply.issues")}</h3>
                    {report.issues.map((issue) => (
                      <MessageBar key={issue.occurrenceId ?? `${issue.id}-${issue.itemId ?? issue.detail}`} intent={issue.severity === "error" ? "error" : issue.severity === "warning" ? "warning" : "info"}>
                        <MessageBarBody>
                          <strong>{t(`apply.notices.${issue.id}.title`, { defaultValue: issue.title })}</strong>{" "}
                          {t(`apply.notices.${issue.id}.detail`, { defaultValue: issue.detail })}
                        </MessageBarBody>
                      </MessageBar>
                    ))}
                  </section>
                )}
                {closeBlocked && confirmForceClose && (
                  <MessageBar intent="warning">
                    <MessageBarBody>
                      <strong>{t("apply.forceConfirmTitle")}</strong> {t("apply.forceConfirmBody")}
                    </MessageBarBody>
                  </MessageBar>
                )}
              </DialogContent>
              <DialogActions>
                {closeBlocked ? (
                  confirmForceClose ? (
                    <>
                      <Button appearance="secondary" onClick={() => setConfirmForceClose(false)}>{t("common.cancel")}</Button>
                      <Button appearance="primary" onClick={retryWithForce}>{t("apply.forceConfirmAction")}</Button>
                    </>
                  ) : (
                    <>
                      <Button appearance="secondary" onClick={close}>{t("apply.leaveBlockedOpen")}</Button>
                      <Button appearance="primary" onClick={() => setConfirmForceClose(true)}>{t("apply.forceClose")}</Button>
                    </>
                  )
                ) : (
                  <>
                    {(report.status === "failed" || report.status === "partial") && <Button appearance="secondary" onClick={() => { close(); onDiagnostics(); }}>{t("apply.viewDiagnostics")}</Button>}
                    <Button appearance="primary" onClick={close}>{t("apply.backToPresets")}</Button>
                  </>
                )}
              </DialogActions>
            </>
          )}
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
