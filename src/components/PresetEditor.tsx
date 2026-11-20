import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  Radio,
  RadioGroup,
  Tab,
  TabList,
  Textarea,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add20Regular,
  ArrowLeft20Regular,
  Delete20Regular,
  Desktop20Regular,
  Save20Regular,
} from "@fluentui/react-icons";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { DesktopPolicy, LaunchItem, MonitorRef, PresetV1, Rect } from "../types";
import { AppInspector } from "./AppInspector";
import { normalizeShortcut, ShortcutInput } from "./ShortcutInput";

interface PresetEditorProps {
  initialPreset: PresetV1;
  onSave: (preset: PresetV1) => Promise<void>;
  onCancel: () => void;
  reservedHotkeys?: string[];
}

interface PointerSession {
  kind: "move" | "resize";
  itemId: string;
  startClientX: number;
  startClientY: number;
  start: Pick<LaunchItem["placement"], "x" | "y" | "width" | "height">;
}

function topologyBounds(monitors: PresetV1["monitors"]): Rect {
  if (monitors.length === 0) return { x: 0, y: 0, width: 1920, height: 1080 };
  const left = Math.min(...monitors.map((monitor) => monitor.bounds.x));
  const top = Math.min(...monitors.map((monitor) => monitor.bounds.y));
  const right = Math.max(...monitors.map((monitor) => monitor.bounds.x + monitor.bounds.width));
  const bottom = Math.max(...monitors.map((monitor) => monitor.bounds.y + monitor.bounds.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

function overlapArea(placement: LaunchItem["placement"], monitor: MonitorRef) {
  const left = Math.max(placement.x, monitor.bounds.x);
  const top = Math.max(placement.y, monitor.bounds.y);
  const right = Math.min(placement.x + placement.width, monitor.bounds.x + monitor.bounds.width);
  const bottom = Math.min(placement.y + placement.height, monitor.bounds.y + monitor.bounds.height);
  return Math.max(0, right - left) * Math.max(0, bottom - top);
}

function placementMonitor(item: LaunchItem, monitors: PresetV1["monitors"]): MonitorRef | undefined {
  const centerX = item.placement.x + item.placement.width / 2;
  const centerY = item.placement.y + item.placement.height / 2;
  const containingCenter = monitors.find((monitor) => (
    centerX >= monitor.bounds.x
    && centerX < monitor.bounds.x + monitor.bounds.width
    && centerY >= monitor.bounds.y
    && centerY < monitor.bounds.y + monitor.bounds.height
  ));
  if (containingCenter) return containingCenter;

  const byOverlap = [...monitors].sort((left, right) => overlapArea(item.placement, right) - overlapArea(item.placement, left));
  if (byOverlap[0] && overlapArea(item.placement, byOverlap[0]) > 0) return byOverlap[0];
  return monitors.find((monitor) => monitor.id === item.placement.monitorId) ?? monitors[0];
}

export function normalizeItemPlacement(item: LaunchItem, monitors: PresetV1["monitors"]): LaunchItem {
  const monitor = placementMonitor(item, monitors);
  if (!monitor || monitor.workArea.width <= 0 || monitor.workArea.height <= 0) return item;
  return {
    ...item,
    placement: {
      ...item.placement,
      monitorId: monitor.id,
      normalizedX: (item.placement.x - monitor.workArea.x) / monitor.workArea.width,
      normalizedY: (item.placement.y - monitor.workArea.y) / monitor.workArea.height,
      normalizedWidth: item.placement.width / monitor.workArea.width,
      normalizedHeight: item.placement.height / monitor.workArea.height,
    },
  };
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), Math.max(minimum, maximum));
}

export function moveItemToMonitor(item: LaunchItem, monitors: PresetV1["monitors"], monitorId: string): LaunchItem {
  const source = monitors.find((monitor) => monitor.id === item.placement.monitorId) ?? monitors[0];
  const target = monitors.find((monitor) => monitor.id === monitorId);
  if (!source || !target || source.workArea.width <= 0 || source.workArea.height <= 0) return item;
  const normalizedX = (item.placement.x - source.workArea.x) / source.workArea.width;
  const normalizedY = (item.placement.y - source.workArea.y) / source.workArea.height;
  const normalizedWidth = item.placement.width / source.workArea.width;
  const normalizedHeight = item.placement.height / source.workArea.height;
  const width = Math.min(target.workArea.width, Math.max(160, Math.round(normalizedWidth * target.workArea.width)));
  const height = Math.min(target.workArea.height, Math.max(120, Math.round(normalizedHeight * target.workArea.height)));
  const x = clamp(
    Math.round(target.workArea.x + normalizedX * target.workArea.width),
    target.workArea.x,
    target.workArea.x + target.workArea.width - width,
  );
  const y = clamp(
    Math.round(target.workArea.y + normalizedY * target.workArea.height),
    target.workArea.y,
    target.workArea.y + target.workArea.height - height,
  );
  return normalizeItemPlacement({
    ...item,
    placement: { ...item.placement, monitorId: target.id, x, y, width, height },
  }, monitors);
}

function fallbackMonitor(): MonitorRef {
  return {
    id: "fallback-primary",
    deviceName: "DISPLAY1",
    friendlyName: "Primary monitor",
    bounds: { x: 0, y: 0, width: 1920, height: 1080 },
    workArea: { x: 0, y: 0, width: 1920, height: 1040 },
    dpiX: 96,
    dpiY: 96,
    primary: true,
  };
}

export function PresetEditor({ initialPreset, onSave, onCancel, reservedHotkeys = [] }: PresetEditorProps) {
  const { t } = useTranslation();
  const [preset, setPreset] = useState(() => structuredClone(initialPreset));
  const [activeDesktopId, setActiveDesktopId] = useState(initialPreset.desktops[0]?.id ?? "");
  const [selectedItemId, setSelectedItemId] = useState<string>();
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [validation, setValidation] = useState("");
  const [confirmCancel, setConfirmCancel] = useState(false);
  const pointer = useRef<PointerSession | undefined>(undefined);
  const canvasRef = useRef<HTMLDivElement>(null);
  const bounds = useMemo(() => topologyBounds(preset.monitors), [preset.monitors]);
  const activeItems = preset.items.filter((item) => item.desktopId === activeDesktopId);
  const selectedItem = preset.items.find((item) => item.id === selectedItemId);
  const hotkeyConflict = Boolean(
    preset.hotkey && reservedHotkeys.some((shortcut) => normalizeShortcut(shortcut) === normalizeShortcut(preset.hotkey ?? "")),
  );

  const mutate = (updater: (current: PresetV1) => PresetV1) => {
    setPreset((current) => updater(current));
    setDirty(true);
    setValidation("");
  };

  const updateItem = (updated: LaunchItem) => {
    mutate((current) => ({
      ...current,
      items: current.items.map((item) => item.id === updated.id ? normalizeItemPlacement(updated, current.monitors) : item),
    }));
  };

  const addDesktop = () => {
    const nextIndex = preset.desktops.length;
    const desktop = { id: crypto.randomUUID(), name: t("editor.desktopName", { index: nextIndex + 1 }), index: nextIndex };
    mutate((current) => ({ ...current, desktops: [...current.desktops, desktop] }));
    setActiveDesktopId(desktop.id);
    setSelectedItemId(undefined);
  };

  const addItem = () => {
    const itemId = crypto.randomUUID();
    mutate((current) => {
      const monitor = current.monitors[0] ?? fallbackMonitor();
      const monitors = current.monitors.length > 0 ? current.monitors : [monitor];
      const desktopId = current.desktops.some((desktop) => desktop.id === activeDesktopId)
        ? activeDesktopId
        : current.desktops[0].id;
      const width = Math.max(320, Math.min(960, monitor.workArea.width - 80));
      const height = Math.max(240, Math.min(720, monitor.workArea.height - 80));
      const x = monitor.workArea.x + Math.max(0, Math.round((monitor.workArea.width - width) / 2));
      const y = monitor.workArea.y + Math.max(0, Math.round((monitor.workArea.height - height) / 2));
      const item: LaunchItem = {
        id: itemId,
        displayName: t("editor.newApp"),
        desktopId,
        launchTarget: { kind: "executable", path: "C:\\Windows\\System32\\notepad.exe", arguments: [] },
        selector: {
          executablePath: "C:\\Windows\\System32\\notepad.exe",
          instanceIndex: 0,
        },
        placement: {
          monitorId: monitor.id,
          x,
          y,
          width,
          height,
          normalizedX: (x - monitor.workArea.x) / monitor.workArea.width,
          normalizedY: (y - monitor.workArea.y) / monitor.workArea.height,
          normalizedWidth: width / monitor.workArea.width,
          normalizedHeight: height / monitor.workArea.height,
          state: "normal",
        },
        lifecycle: "reuse",
        enabled: true,
      };
      return { ...current, monitors, items: [...current.items, item] };
    });
    setSelectedItemId(itemId);
  };

  const removeDesktop = () => {
    if (preset.desktops.length <= 1) return;
    const nextDesktops = preset.desktops.filter((desktop) => desktop.id !== activeDesktopId).map((desktop, index) => ({ ...desktop, index }));
    const fallbackId = nextDesktops[0].id;
    mutate((current) => ({
      ...current,
      desktops: nextDesktops,
      items: current.items.map((item) => item.desktopId === activeDesktopId ? { ...item, desktopId: fallbackId } : item),
    }));
    setActiveDesktopId(fallbackId);
    setSelectedItemId(undefined);
  };

  const pointerDown = (event: React.PointerEvent, item: LaunchItem, kind: PointerSession["kind"]) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    pointer.current = {
      kind,
      itemId: item.id,
      startClientX: event.clientX,
      startClientY: event.clientY,
      start: {
        x: item.placement.x,
        y: item.placement.y,
        width: item.placement.width,
        height: item.placement.height,
      },
    };
    setSelectedItemId(item.id);
  };

  const pointerMove = (event: React.PointerEvent) => {
    const session = pointer.current;
    const canvas = canvasRef.current;
    if (!session || !canvas) return;
    const item = preset.items.find((entry) => entry.id === session.itemId);
    if (!item) return;
    const rect = canvas.getBoundingClientRect();
    const dx = ((event.clientX - session.startClientX) / rect.width) * bounds.width;
    const dy = ((event.clientY - session.startClientY) / rect.height) * bounds.height;
    if (session.kind === "move") {
      updateItem({
        ...item,
        placement: {
          ...item.placement,
          x: Math.round(session.start.x + dx),
          y: Math.round(session.start.y + dy),
        },
      });
    } else {
      updateItem({
        ...item,
        placement: {
          ...item.placement,
          width: Math.max(160, Math.round(session.start.width + dx)),
          height: Math.max(120, Math.round(session.start.height + dy)),
        },
      });
    }
  };

  const moveWithKeyboard = (event: React.KeyboardEvent, item: LaunchItem) => {
    const amount = event.shiftKey ? 10 : 1;
    const changes: Record<string, number> = {};
    if (event.key === "ArrowLeft") changes.x = item.placement.x - amount;
    else if (event.key === "ArrowRight") changes.x = item.placement.x + amount;
    else if (event.key === "ArrowUp") changes.y = item.placement.y - amount;
    else if (event.key === "ArrowDown") changes.y = item.placement.y + amount;
    else return;
    event.preventDefault();
    updateItem({ ...item, placement: { ...item.placement, ...changes } });
  };

  const save = async () => {
    if (!preset.name.trim()) {
      setValidation(t("editor.requiredName"));
      return;
    }
    if (preset.desktops.length === 0) {
      setValidation(t("editor.requiredDesktop"));
      return;
    }
    if (hotkeyConflict) {
      setValidation(t("editor.hotkeyConflict"));
      return;
    }
    setSaving(true);
    try {
      await onSave({ ...preset, name: preset.name.trim() });
      setDirty(false);
    } finally {
      setSaving(false);
    }
  };

  const requestCancel = () => dirty ? setConfirmCancel(true) : onCancel();

  return (
    <div className="editor-view">
      <header className="editor-header">
        <Button appearance="subtle" icon={<ArrowLeft20Regular />} onClick={requestCancel}>{t("common.back")}</Button>
        <div>
          <h1>{initialPreset.items.length === 0 && !initialPreset.description ? t("editor.titleNew") : t("editor.titleEdit", { name: initialPreset.name })}</h1>
          <p>{t("editor.subtitle")}</p>
        </div>
        <Button appearance="primary" icon={<Save20Regular />} disabled={saving || hotkeyConflict} onClick={() => void save()}>
          {saving ? t("common.saving") : t("common.save")}
        </Button>
      </header>

      <div className={`editor-layout${selectedItem ? " has-inspector" : ""}`}>
        <main className="editor-main">
          <section className="editor-metadata" aria-label={t("editor.titleEdit", { name: preset.name })}>
            <div className="field-grid field-grid--editor">
              <Field label={t("editor.name")} required validationMessage={validation}>
                <Input value={preset.name} onChange={(_, data) => mutate((current) => ({ ...current, name: data.value }))} />
              </Field>
              <Field label={t("editor.description")}>
                <Textarea
                  resize="vertical"
                  value={preset.description ?? ""}
                  placeholder={t("editor.descriptionPlaceholder")}
                  onChange={(_, data) => mutate((current) => ({ ...current, description: data.value }))}
                />
              </Field>
              <Field
                label={t("editor.hotkey")}
                validationState={hotkeyConflict ? "error" : "none"}
                validationMessage={hotkeyConflict ? t("editor.hotkeyConflict") : undefined}
              >
                <ShortcutInput
                  value={preset.hotkey ?? ""}
                  placeholder={t("editor.hotkeyPlaceholder")}
                  onChange={(hotkey) => mutate((current) => ({ ...current, hotkey: hotkey || undefined }))}
                />
              </Field>
            </div>
            <div className="policy-field">
              <strong>{t("editor.policy")}</strong>
              <RadioGroup
                layout="horizontal"
                value={preset.desktopPolicy}
                onChange={(_, data) => mutate((current) => ({ ...current, desktopPolicy: data.value as DesktopPolicy }))}
              >
                <Radio value="exactProtected" label={<span><strong>{t("editor.exactProtected")}</strong><small>{t("editor.exactProtectedHint")}</small></span>} />
                <Radio value="managedOnly" label={<span><strong>{t("editor.managedOnly")}</strong><small>{t("editor.managedOnlyHint")}</small></span>} />
              </RadioGroup>
            </div>
          </section>

          <section className="desktop-workbench" aria-labelledby="topology-title">
            <div className="desktop-toolbar">
              <TabList
                selectedValue={activeDesktopId}
                onTabSelect={(_, data) => {
                  setActiveDesktopId(String(data.value));
                  setSelectedItemId(undefined);
                }}
              >
                {preset.desktops.map((desktop) => (
                  <Tab key={desktop.id} value={desktop.id} icon={<Desktop20Regular />}>{desktop.name}</Tab>
                ))}
              </TabList>
              <div>
                <Button appearance="secondary" icon={<Add20Regular />} onClick={addItem}>{t("editor.addApp")}</Button>
                <Tooltip content={t("editor.removeDesktopBlocked")} relationship="description">
                  <Button
                    appearance="subtle"
                    icon={<Delete20Regular />}
                    aria-label={t("editor.removeDesktop")}
                    disabled={preset.desktops.length <= 1}
                    onClick={removeDesktop}
                  />
                </Tooltip>
                <Button appearance="subtle" icon={<Add20Regular />} onClick={addDesktop}>{t("editor.addDesktop")}</Button>
              </div>
            </div>

            <div className="topology-heading">
              <div>
                <h2 id="topology-title">{t("editor.topology")}</h2>
                <p>{t("editor.canvasHelp")}</p>
              </div>
              <span>{bounds.width} × {bounds.height} px</span>
            </div>

            <div className="monitor-canvas-wrap">
              <div
                ref={canvasRef}
                className="monitor-canvas"
                style={{ aspectRatio: `${bounds.width} / ${bounds.height}` }}
                onPointerMove={pointerMove}
                onPointerUp={() => { pointer.current = undefined; }}
                onPointerCancel={() => { pointer.current = undefined; }}
              >
                {preset.monitors.map((monitor) => (
                  <div
                    key={monitor.id}
                    className="monitor-frame"
                    style={{
                      left: `${((monitor.bounds.x - bounds.x) / bounds.width) * 100}%`,
                      top: `${((monitor.bounds.y - bounds.y) / bounds.height) * 100}%`,
                      width: `${(monitor.bounds.width / bounds.width) * 100}%`,
                      height: `${(monitor.bounds.height / bounds.height) * 100}%`,
                    }}
                  >
                    <span className="monitor-label">
                      {monitor.friendlyName}
                      {monitor.primary && <small>{t("editor.monitorPrimary")}</small>}
                    </span>
                  </div>
                ))}
                {activeItems.map((item, index) => (
                  <button
                    type="button"
                    key={item.id}
                    className={`window-card window-card--${index % 4}${selectedItemId === item.id ? " is-selected" : ""}${!item.enabled ? " is-disabled" : ""}`}
                    style={{
                      left: `${((item.placement.x - bounds.x) / bounds.width) * 100}%`,
                      top: `${((item.placement.y - bounds.y) / bounds.height) * 100}%`,
                      width: `${(item.placement.width / bounds.width) * 100}%`,
                      height: `${(item.placement.height / bounds.height) * 100}%`,
                    }}
                    aria-label={`${item.displayName}, ${item.placement.x}, ${item.placement.y}, ${item.placement.width} × ${item.placement.height}`}
                    aria-pressed={selectedItemId === item.id}
                    onPointerDown={(event) => pointerDown(event, item, "move")}
                    onKeyDown={(event) => moveWithKeyboard(event, item)}
                    onClick={() => setSelectedItemId(item.id)}
                  >
                    <span className="window-card__bar"><i /><i /><i /></span>
                    <span className="window-card__content">
                      <b className="app-avatar" aria-hidden="true">{item.displayName.slice(0, 1)}</b>
                      <strong>{item.displayName}</strong>
                      <small>{t(`inspector.${item.placement.state}`)}</small>
                    </span>
                    <span
                      className="window-card__resize"
                      aria-hidden="true"
                      onPointerDown={(event) => {
                        event.stopPropagation();
                        pointerDown(event, item, "resize");
                      }}
                    />
                  </button>
                ))}
              </div>
            </div>
            {activeItems.length === 0 && <p className="empty-inline empty-inline--workbench">{t("editor.noApps")}</p>}
          </section>
        </main>

        {selectedItem && (
          <AppInspector
            item={selectedItem}
            desktops={preset.desktops}
            monitors={preset.monitors}
            onChange={updateItem}
            onMonitorChange={(monitorId) => updateItem(moveItemToMonitor(selectedItem, preset.monitors, monitorId))}
            onClose={() => setSelectedItemId(undefined)}
            onRemove={() => {
              mutate((current) => ({ ...current, items: current.items.filter((item) => item.id !== selectedItem.id) }));
              setSelectedItemId(undefined);
            }}
          />
        )}
      </div>

      <Dialog open={confirmCancel} onOpenChange={(_, data) => setConfirmCancel(data.open)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>{t("editor.unsavedTitle")}</DialogTitle>
            <DialogContent>{t("editor.unsavedBody")}</DialogContent>
            <DialogActions>
              <Button appearance="secondary" onClick={() => setConfirmCancel(false)}>{t("common.cancel")}</Button>
              <Button appearance="primary" onClick={onCancel}>{t("editor.discard")}</Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  );
}
