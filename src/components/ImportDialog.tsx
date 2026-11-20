import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  Spinner,
  Textarea,
} from "@fluentui/react-components";
import { CheckmarkCircle20Regular, SaveArrowRight20Regular } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { worksetsApi } from "../api";
import type { ImportResult, PresetV1 } from "../types";

interface ImportDialogProps {
  open: boolean;
  onClose: () => void;
  onImported: (preset: PresetV1) => void;
}

function failedImport(reason: unknown): ImportResult {
  const message = reason instanceof Error
    ? reason.message
    : typeof reason === "string"
      ? reason
      : reason && typeof reason === "object" && "message" in reason && typeof reason.message === "string"
        ? reason.message
        : String(reason);
  return { valid: false, warnings: [], errors: [message], committed: false };
}

export function ImportDialog({ open, onClose, onImported }: ImportDialogProps) {
  const { t } = useTranslation();
  const [json, setJson] = useState("");
  const [result, setResult] = useState<ImportResult>();
  const [loading, setLoading] = useState(false);

  const preview = async () => {
    setLoading(true);
    try {
      setResult(await worksetsApi.importPreset(json, false));
    } catch (reason) {
      setResult(failedImport(reason));
    } finally {
      setLoading(false);
    }
  };

  const commit = async () => {
    setLoading(true);
    try {
      const imported = await worksetsApi.importPreset(json, true);
      setResult(imported);
      if (imported.valid && imported.preset) {
        onImported(imported.preset);
        setJson("");
        setResult(undefined);
      }
    } catch (reason) {
      setResult(failedImport(reason));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("importExport.title")}</DialogTitle>
          <DialogContent className="import-content">
            <Textarea
              aria-label={t("importExport.paste")}
              placeholder="{\n  &quot;schemaVersion&quot;: 1,\n  …\n}"
              resize="vertical"
              value={json}
              onChange={(_, data) => {
                setJson(data.value);
                setResult(undefined);
              }}
            />
            {loading && <Spinner size="small" label={t("importExport.loading")} />}
            {result?.valid && (
              <MessageBar intent="success"><MessageBarBody><CheckmarkCircle20Regular aria-hidden /> {t("importExport.valid")}</MessageBarBody></MessageBar>
            )}
            {result?.warnings.map((warning) => (
              <MessageBar key={`${warning.code}-${warning.message}`} intent={warning.severity}>
                <MessageBarBody>{t(`importExport.notice.${warning.code}`, { defaultValue: warning.message })}</MessageBarBody>
              </MessageBar>
            ))}
            {result && !result.valid && (
              <MessageBar intent="error"><MessageBarBody>{t("importExport.invalid")} {result.errors.join(" ")}</MessageBarBody></MessageBar>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onClose}>{t("common.cancel")}</Button>
            {!result?.valid ? (
              <Button appearance="primary" disabled={!json.trim() || loading} onClick={() => void preview()}>{t("importExport.preview")}</Button>
            ) : (
              <Button appearance="primary" icon={<SaveArrowRight20Regular />} disabled={loading} onClick={() => void commit()}>{t("importExport.commit")}</Button>
            )}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
