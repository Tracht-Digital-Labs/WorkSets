import {
  Badge,
  Button,
  Card,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add20Regular,
  Apps20Regular,
  ArrowSync20Regular,
  Copy20Regular,
  Delete20Regular,
  Desktop20Regular,
  Edit20Regular,
  MoreHorizontal20Regular,
  Open20Regular,
  SaveArrowRight20Regular,
  WindowMultiple20Regular,
} from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ApplyReport, PresetV1, UpdateInfo } from "../types";
import { WorkSetsMark } from "./Brand";

interface DashboardProps {
  presets: PresetV1[];
  lastApplyReport?: ApplyReport;
  updateInfo?: UpdateInfo;
  onCreate: () => void;
  onCapture: () => void;
  onEdit: (preset: PresetV1) => void;
  onDuplicate: (preset: PresetV1) => Promise<void>;
  onDelete: (preset: PresetV1) => Promise<void>;
  onApply: (preset: PresetV1) => void;
  onExport: (preset: PresetV1) => Promise<void>;
  onImport: () => void;
  onOpenUpdate: (url: string) => void;
  onDismissUpdate: () => void;
}

export function Dashboard({
  presets,
  lastApplyReport,
  updateInfo,
  onCreate,
  onCapture,
  onEdit,
  onDuplicate,
  onDelete,
  onApply,
  onExport,
  onImport,
  onOpenUpdate,
  onDismissUpdate,
}: DashboardProps) {
  const { t, i18n } = useTranslation();
  const [deleteTarget, setDeleteTarget] = useState<PresetV1>();
  const [deleting, setDeleting] = useState(false);
  const releaseUrl = updateInfo?.releaseUrl;

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await onDelete(deleteTarget);
      setDeleteTarget(undefined);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="view dashboard-view">
      {updateInfo?.available && updateInfo.latestVersion && releaseUrl && (
        <section className="update-banner" aria-labelledby="update-title">
          <div className="update-banner__mark"><WorkSetsMark size={38} decorative /></div>
          <div>
            <strong id="update-title">{t("update.title", { version: updateInfo.latestVersion })}</strong>
            <span>{t("update.body")}</span>
          </div>
          <Button appearance="secondary" icon={<Open20Regular />} onClick={() => onOpenUpdate(releaseUrl)}>
            {t("update.openRelease")}
          </Button>
          <Button appearance="subtle" aria-label={t("common.close")} onClick={onDismissUpdate}>×</Button>
        </section>
      )}

      <header className="view-header dashboard-header">
        <div>
          <p className="eyebrow">{t("dashboard.eyebrow")}</p>
          <h1>{t("dashboard.title")}</h1>
          <p>{t("dashboard.subtitle")}</p>
        </div>
        <div className="view-header__actions">
          <Menu>
            <MenuTrigger disableButtonEnhancement>
              <Button appearance="secondary" icon={<MoreHorizontal20Regular />} aria-label={t("common.menu")} />
            </MenuTrigger>
            <MenuPopover>
              <MenuList>
                <MenuItem icon={<SaveArrowRight20Regular />} onClick={onImport}>{t("importExport.import")}</MenuItem>
              </MenuList>
            </MenuPopover>
          </Menu>
          <Button appearance="secondary" icon={<WindowMultiple20Regular />} onClick={onCapture}>
            {t("dashboard.capture")}
          </Button>
          <Button appearance="primary" icon={<Add20Regular />} onClick={onCreate}>
            {t("dashboard.newPreset")}
          </Button>
        </div>
      </header>

      {presets.length === 0 ? (
        <section className="empty-state">
          <div className="empty-state__visual" aria-hidden="true">
            <span />
            <span />
            <WorkSetsMark size={52} decorative />
          </div>
          <h2>{t("dashboard.emptyTitle")}</h2>
          <p>{t("dashboard.emptyBody")}</p>
          <div className="empty-state__actions">
            <Button appearance="primary" icon={<WindowMultiple20Regular />} onClick={onCapture}>
              {t("dashboard.capture")}
            </Button>
            <Button appearance="secondary" icon={<Add20Regular />} onClick={onCreate}>
              {t("dashboard.newPreset")}
            </Button>
          </div>
        </section>
      ) : (
        <section className="preset-grid" aria-label={t("nav.presets")}>
          {presets.map((preset) => {
            const enabledApps = preset.items.filter((item) => item.enabled);
            const latestForPreset = lastApplyReport?.presetId === preset.id ? lastApplyReport : undefined;
            const needsAttention = preset.items.some((item) => item.elevated) || latestForPreset?.status === "partial";
            return (
              <Card key={preset.id} className="preset-card" appearance="outline">
                <div className="preset-card__topline">
                  <div className="preset-card__swatch" aria-hidden="true">
                    <span />
                    <span />
                    <span />
                  </div>
                  <Badge appearance="tint" color={needsAttention ? "warning" : "success"}>
                    {needsAttention ? t("dashboard.attention") : t("dashboard.healthy")}
                  </Badge>
                  <Menu>
                    <MenuTrigger disableButtonEnhancement>
                      <Tooltip content={t("dashboard.cardMenu", { name: preset.name })} relationship="label">
                        <Button
                          appearance="subtle"
                          size="small"
                          icon={<MoreHorizontal20Regular />}
                          aria-label={t("dashboard.cardMenu", { name: preset.name })}
                        />
                      </Tooltip>
                    </MenuTrigger>
                    <MenuPopover>
                      <MenuList>
                        <MenuItem icon={<Edit20Regular />} onClick={() => onEdit(preset)}>{t("common.edit")}</MenuItem>
                        <MenuItem icon={<Copy20Regular />} onClick={() => void onDuplicate(preset)}>{t("common.duplicate")}</MenuItem>
                        <MenuItem icon={<SaveArrowRight20Regular />} onClick={() => void onExport(preset)}>{t("importExport.export")}</MenuItem>
                        <MenuItem icon={<Delete20Regular />} onClick={() => setDeleteTarget(preset)}>{t("common.delete")}</MenuItem>
                      </MenuList>
                    </MenuPopover>
                  </Menu>
                </div>
                <div className="preset-card__body">
                  <h2>{preset.name}</h2>
                  <Text>{preset.description || t("dashboard.noDescription")}</Text>
                </div>
                <dl className="preset-stats">
                  <div>
                    <dt><Desktop20Regular aria-hidden /></dt>
                    <dd>{t("dashboard.desktops", { count: preset.desktops.length })}</dd>
                  </div>
                  <div>
                    <dt><Apps20Regular aria-hidden /></dt>
                    <dd>{t("dashboard.apps", { count: enabledApps.length })}</dd>
                  </div>
                  <div>
                    <dt><WindowMultiple20Regular aria-hidden /></dt>
                    <dd>{t("dashboard.monitors", { count: preset.monitors.length })}</dd>
                  </div>
                </dl>
                <div className="preset-card__footer">
                  <span>{t("dashboard.updated", { date: new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(new Date(preset.updatedAt)) })}</span>
                  <Button appearance="primary" icon={<ArrowSync20Regular />} onClick={() => onApply(preset)}>
                    {t("dashboard.apply")}
                  </Button>
                </div>
              </Card>
            );
          })}
        </section>
      )}

      <Dialog open={Boolean(deleteTarget)} onOpenChange={(_, data) => !data.open && setDeleteTarget(undefined)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>{t("dashboard.deleteTitle")}</DialogTitle>
            <DialogContent>{t("dashboard.deleteBody", { name: deleteTarget?.name })}</DialogContent>
            <DialogActions>
              <Button appearance="secondary" disabled={deleting} onClick={() => setDeleteTarget(undefined)}>{t("common.cancel")}</Button>
              <Button appearance="primary" disabled={deleting} onClick={() => void confirmDelete()}>{t("common.delete")}</Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  );
}
