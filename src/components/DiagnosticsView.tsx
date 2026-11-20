import { Badge, Button, MessageBar, MessageBarBody, Spinner } from "@fluentui/react-components";
import { ArrowClockwise20Regular, Checkmark20Regular, Copy20Regular, ErrorCircle20Regular, Warning20Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { worksetsApi } from "../api";
import type { Diagnostics } from "../types";

export function DiagnosticsView() {
  const { t, i18n } = useTranslation();
  const [diagnostics, setDiagnostics] = useState<Diagnostics>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setDiagnostics(await worksetsApi.getDiagnostics());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const copy = async () => {
    if (!diagnostics) return;
    const sanitized = {
      appVersion: diagnostics.appVersion,
      generatedAt: diagnostics.generatedAt,
      brokerProtocol: diagnostics.brokerProtocol,
      sessionMutationsEnabled: diagnostics.sessionMutationsEnabled,
      checks: diagnostics.checks.map(({ id, label, status, detail }) => ({ id, label, status, detail })),
    };
    await navigator.clipboard.writeText(JSON.stringify(sanitized, null, 2));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 2000);
  };

  const checkLabel = (id: string, fallback: string) => t(`diagnostics.checks.${id}.label`, { defaultValue: fallback });
  const checkDetail = (check: Diagnostics["checks"][number]) => check.detailKey
    ? t(`diagnostics.checks.${check.id}.${check.detailKey}`, { ...check.detailValues, defaultValue: check.detail })
    : check.detail;

  return (
    <div className="view diagnostics-view">
      <header className="view-header">
        <div><h1>{t("diagnostics.title")}</h1><p>{t("diagnostics.subtitle")}</p></div>
        <div className="view-header__actions">
          <Button appearance="secondary" icon={<Copy20Regular />} disabled={!diagnostics} onClick={() => void copy()}>{t("diagnostics.copy")}</Button>
          <Button appearance="primary" icon={<ArrowClockwise20Regular />} disabled={loading} onClick={() => void load()}>{t("diagnostics.refresh")}</Button>
        </div>
      </header>
      {copied && <MessageBar intent="success"><MessageBarBody>{t("diagnostics.copied")}</MessageBarBody></MessageBar>}
      {error && <MessageBar intent="error"><MessageBarBody>{error}</MessageBarBody></MessageBar>}
      {loading ? (
        <div className="loading-state"><Spinner size="medium" /><span>{t("diagnostics.loading")}</span></div>
      ) : diagnostics ? (
        <>
          <section className="diagnostic-summary">
            <article><small>{t("diagnostics.appVersion")}</small><strong>{diagnostics.appVersion}</strong></article>
            <article><small>{t("diagnostics.broker")}</small><strong>{diagnostics.brokerProtocol}</strong></article>
            <article><small>{t("diagnostics.mutations")}</small><strong>{diagnostics.sessionMutationsEnabled ? t("diagnostics.enabled") : t("diagnostics.disabled")}</strong></article>
          </section>
          <section className="diagnostic-checks" aria-label={t("diagnostics.title")}>
            {diagnostics.checks.map((check) => (
              <article key={check.id} className={`diagnostic-check diagnostic-check--${check.status}`}>
                <span className="diagnostic-check__icon">
                  {check.status === "ok" ? <Checkmark20Regular aria-hidden /> : check.status === "warning" ? <Warning20Regular aria-hidden /> : <ErrorCircle20Regular aria-hidden />}
                </span>
                <div><strong>{checkLabel(check.id, check.label)}</strong><p>{checkDetail(check)}</p></div>
                <Badge appearance="tint" color={check.status === "ok" ? "success" : check.status === "warning" ? "warning" : "danger"}>{t(`diagnostics.status.${check.status}`)}</Badge>
              </article>
            ))}
          </section>
          <section className="diagnostic-path">
            <div><small>{t("diagnostics.storage")}</small><code>{diagnostics.storagePath}</code></div>
            <span>{t("diagnostics.generated", { date: new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(new Date(diagnostics.generatedAt)) })}</span>
          </section>
          <p className="privacy-inline">{t("diagnostics.privacy")}</p>
        </>
      ) : null}
    </div>
  );
}
