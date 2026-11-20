import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  MessageBar,
  MessageBarBody,
  Spinner,
} from "@fluentui/react-components";
import { ArrowClockwise20Regular, ShieldLock20Regular, WindowMultiple20Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { worksetsApi } from "../api";
import type { CaptureResult, PresetV1 } from "../types";

interface CaptureDialogProps {
  open: boolean;
  onClose: () => void;
  onContinue: (preset: PresetV1) => void;
}

export function CaptureDialog({ open, onClose, onContinue }: CaptureDialogProps) {
  const { t } = useTranslation();
  const [capture, setCapture] = useState<CaptureResult>();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [validation, setValidation] = useState("");
  const selectable = useMemo(() => capture?.preset.items.filter((item) => !item.elevated) ?? [], [capture]);
  const warningText = (code: string, fallback: string) => t(`capture.notice.${code}`, { defaultValue: fallback });

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    setValidation("");
    try {
      const result = await worksetsApi.captureWorkspace();
      setCapture(result);
      setName(result.preset.name);
      setSelectedIds(new Set(result.preset.items.filter((item) => !item.elevated && item.enabled).map((item) => item.id)));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const continueToEditor = () => {
    if (!capture) return;
    if (!name.trim()) {
      setValidation(t("capture.validationName"));
      return;
    }
    if (selectedIds.size === 0) {
      setValidation(t("capture.validationWindow"));
      return;
    }
    const preset: PresetV1 = {
      ...capture.preset,
      name: name.trim(),
      items: capture.preset.items.filter((item) => selectedIds.has(item.id)).map((item) => ({ ...item, enabled: true })),
    };
    onContinue(preset);
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface className="capture-dialog">
        <DialogBody className="capture-body">
          <DialogTitle>{t("capture.title")}</DialogTitle>
          <DialogContent className="capture-content">
            <p className="dialog-subtitle">{t("capture.subtitle")}</p>
            {loading ? (
              <div className="loading-state">
                <Spinner size="medium" />
                <span>{t("capture.loading")}</span>
              </div>
            ) : error ? (
              <div className="error-state">
                <MessageBar intent="error"><MessageBarBody>{error}</MessageBarBody></MessageBar>
                <Button icon={<ArrowClockwise20Regular />} onClick={() => void load()}>{t("capture.retry")}</Button>
              </div>
            ) : capture ? (
              <>
                <Field label={t("capture.nameLabel")} required validationMessage={validation.includes(t("capture.validationName")) ? validation : undefined}>
                  <Input
                    autoFocus
                    value={name}
                    placeholder={t("capture.namePlaceholder")}
                    onChange={(_, data) => {
                      setName(data.value);
                      setValidation("");
                    }}
                  />
                </Field>

                {capture.warnings.map((warning) => (
                  <MessageBar key={`${warning.code}-${warning.message}`} intent={warning.severity}>
                    <MessageBarBody>{warningText(warning.code, warning.message)}</MessageBarBody>
                  </MessageBar>
                ))}

                <div className="capture-toolbar">
                  <strong>{t("capture.selectedCount", { selected: selectedIds.size, total: selectable.length })}</strong>
                  <Button
                    appearance="subtle"
                    size="small"
                    onClick={() => setSelectedIds(selectedIds.size === selectable.length ? new Set() : new Set(selectable.map((item) => item.id)))}
                  >
                    {selectedIds.size === selectable.length ? t("capture.deselectAll") : t("capture.selectAll")}
                  </Button>
                </div>

                <fieldset className="captured-window-list" aria-label={t("capture.title")}>
                  {capture.preset.items.length === 0 && <p className="empty-inline">{t("capture.noWindows")}</p>}
                  {capture.preset.items.map((item) => {
                    const desktop = capture.preset.desktops.find((entry) => entry.id === item.desktopId);
                    const monitor = capture.preset.monitors.find((entry) => entry.id === item.placement.monitorId);
                    return (
                      <label htmlFor={`capture-window-${item.id}`} key={item.id} className={`captured-window${item.elevated ? " is-disabled" : ""}`}>
                        <Checkbox
                          id={`capture-window-${item.id}`}
                          checked={selectedIds.has(item.id)}
                          disabled={item.elevated}
                          aria-label={item.displayName}
                          onChange={(_, data) => {
                            setValidation("");
                            setSelectedIds((current) => {
                              const next = new Set(current);
                              if (data.checked) next.add(item.id);
                              else next.delete(item.id);
                              return next;
                            });
                          }}
                        />
                        <span className="app-avatar" aria-hidden="true">{item.displayName.slice(0, 1)}</span>
                        <span className="captured-window__copy">
                          <strong>{item.displayName}</strong>
                          <small>{desktop?.name} · {monitor?.friendlyName}</small>
                          {item.elevated && <em><ShieldLock20Regular aria-hidden /> {t("capture.inaccessible")}</em>}
                        </span>
                        <span className="window-state-label">{t(`inspector.${item.placement.state}`)}</span>
                      </label>
                    );
                  })}
                </fieldset>
                <p className="privacy-inline"><ShieldLock20Regular aria-hidden /> {t("capture.systemExcluded")}</p>
                {validation && !validation.includes(t("capture.validationName")) && (
                  <MessageBar intent="error"><MessageBarBody>{validation}</MessageBarBody></MessageBar>
                )}
              </>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onClose}>{t("common.cancel")}</Button>
            <Button appearance="primary" icon={<WindowMultiple20Regular />} disabled={loading || !capture} onClick={continueToEditor}>
              {t("capture.openEditor")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
