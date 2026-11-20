import {
  Button,
  Field,
  Input,
  Radio,
  RadioGroup,
  Select,
  Switch,
  Textarea,
} from "@fluentui/react-components";
import { Delete20Regular, Dismiss20Regular, ShieldLock20Regular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { DesktopSpec, LaunchItem, LaunchTarget, MonitorRef } from "../types";

interface AppInspectorProps {
  item: LaunchItem;
  desktops: DesktopSpec[];
  monitors: MonitorRef[];
  onChange: (item: LaunchItem) => void;
  onMonitorChange: (monitorId: string) => void;
  onClose: () => void;
  onRemove: () => void;
}

function targetValue(target: LaunchTarget) {
  if (target.kind === "executable" || target.kind === "file") return target.path;
  if (target.kind === "packagedApp") return target.aumid;
  return target.url;
}

function setTargetValue(target: LaunchTarget, value: string): LaunchTarget {
  switch (target.kind) {
    case "executable": return { ...target, path: value };
    case "packagedApp": return { ...target, aumid: value };
    case "file": return { ...target, path: value };
    case "url": return { ...target, url: value };
  }
}

function newTarget(kind: LaunchTarget["kind"], current: LaunchTarget): LaunchTarget {
  switch (kind) {
    case "executable": return current.kind === kind ? current : { kind, path: "C:\\Windows\\System32\\notepad.exe", arguments: [] };
    case "packagedApp": return current.kind === kind ? current : { kind, aumid: "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", arguments: [] };
    case "file": return current.kind === kind ? current : { kind, path: "C:\\Users\\Public\\Documents\\document.txt" };
    case "url": return current.kind === kind ? current : { kind, url: "https://example.com" };
  }
}

function setTargetArguments(target: LaunchTarget, value: string): LaunchTarget {
  const argumentsList = value.split("\n").filter(Boolean);
  if (target.kind === "executable" || target.kind === "packagedApp") {
    return { ...target, arguments: argumentsList };
  }
  return target;
}

export function AppInspector({ item, desktops, monitors, onChange, onMonitorChange, onClose, onRemove }: AppInspectorProps) {
  const { t } = useTranslation();
  const update = <K extends keyof LaunchItem>(key: K, value: LaunchItem[K]) => onChange({ ...item, [key]: value });
  const updatePlacement = (key: keyof LaunchItem["placement"], value: string | number) => {
    onChange({ ...item, placement: { ...item.placement, [key]: value } });
  };
  const number = (value: string, fallback: number) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? Math.round(parsed) : fallback;
  };
  const updateTarget = (launchTarget: LaunchTarget) => {
    onChange({
      ...item,
      launchTarget,
      selector: {
        ...item.selector,
        executablePath: launchTarget.kind === "executable" ? launchTarget.path : undefined,
        aumid: launchTarget.kind === "packagedApp" ? launchTarget.aumid : undefined,
      },
    });
  };

  return (
    <aside className="app-inspector" aria-labelledby="inspector-title">
      <header className="app-inspector__header">
        <div className="app-avatar app-avatar--large" aria-hidden="true">{item.displayName.slice(0, 1)}</div>
        <div>
          <small>{t("inspector.title")}</small>
          <h2 id="inspector-title">{item.displayName}</h2>
        </div>
        <Button appearance="subtle" icon={<Dismiss20Regular />} aria-label={t("common.close")} onClick={onClose} />
      </header>

      <div className="app-inspector__body">
        {item.elevated && <p className="elevated-warning"><ShieldLock20Regular aria-hidden /> {t("inspector.elevated")}</p>}

        <Switch
          checked={item.enabled}
          disabled={item.elevated}
          label={t("inspector.enabled")}
          onChange={(_, data) => update("enabled", data.checked)}
        />

        <section className="inspector-section">
          <h3>{t("inspector.target")}</h3>
          <Field label={t("inspector.displayName")}>
            <Input value={item.displayName} onChange={(_, data) => update("displayName", data.value)} />
          </Field>
          <Field label={t("inspector.targetKind")}>
            <Select
              value={item.launchTarget.kind}
              onChange={(event) => updateTarget(newTarget(event.target.value as LaunchTarget["kind"], item.launchTarget))}
            >
              <option value="executable">{t("inspector.executable")}</option>
              <option value="packagedApp">{t("inspector.packagedApp")}</option>
              <option value="file">{t("inspector.file")}</option>
              <option value="url">{t("inspector.url")}</option>
            </Select>
          </Field>
          <Field label={t("inspector.path")}>
            <Input value={targetValue(item.launchTarget)} onChange={(_, data) => updateTarget(setTargetValue(item.launchTarget, data.value))} />
          </Field>
          {(item.launchTarget.kind === "executable" || item.launchTarget.kind === "packagedApp") && (
            <Field label={t("inspector.arguments")} hint={t("inspector.argumentsHint")}>
              <Textarea
                resize="vertical"
                value={item.launchTarget.arguments.join("\n")}
                onChange={(_, data) => updateTarget(setTargetArguments(item.launchTarget, data.value))}
              />
            </Field>
          )}
        </section>

        <section className="inspector-section">
          <h3>{t("inspector.lifecycle")}</h3>
          <RadioGroup value={item.lifecycle} onChange={(_, data) => update("lifecycle", data.value as LaunchItem["lifecycle"])}>
            <Radio value="reuse" label={<span><strong>{t("inspector.reuse")}</strong><small>{t("inspector.reuseHint")}</small></span>} />
            <Radio value="restart" label={<span><strong>{t("inspector.restart")}</strong><small>{t("inspector.restartHint")}</small></span>} />
            <Radio value="leaveOpen" label={<span><strong>{t("inspector.leaveOpen")}</strong><small>{t("inspector.leaveOpenHint")}</small></span>} />
          </RadioGroup>
        </section>

        <section className="inspector-section">
          <h3>{t("inspector.geometry")}</h3>
          <div className="field-grid field-grid--two">
            <Field label={t("inspector.desktop")}>
              <Select value={item.desktopId} onChange={(event) => update("desktopId", event.target.value)}>
                {desktops.map((desktop) => <option key={desktop.id} value={desktop.id}>{desktop.name}</option>)}
              </Select>
            </Field>
            <Field label={t("inspector.monitor")}>
              <Select value={item.placement.monitorId} onChange={(event) => onMonitorChange(event.target.value)}>
                {monitors.map((monitor) => <option key={monitor.id} value={monitor.id}>{monitor.friendlyName}</option>)}
              </Select>
            </Field>
            <Field label={t("inspector.state")}>
              <Select value={item.placement.state} onChange={(event) => updatePlacement("state", event.target.value)}>
                <option value="normal">{t("inspector.normal")}</option>
                <option value="maximized">{t("inspector.maximized")}</option>
                <option value="minimized">{t("inspector.minimized")}</option>
              </Select>
            </Field>
          </div>
          <div className="field-grid field-grid--four">
            {(["x", "y", "width", "height"] as const).map((key) => (
              <Field key={key} label={t(`inspector.${key}`)}>
                <Input
                  type="number"
                  min={key === "width" || key === "height" ? 160 : undefined}
                  value={String(item.placement[key])}
                  onChange={(_, data) => updatePlacement(key, number(data.value, item.placement[key]))}
                />
              </Field>
            ))}
          </div>
        </section>

        <section className="inspector-section">
          <h3>{t("inspector.matching")}</h3>
          <Field label={t("inspector.titleContains")}>
            <Input
              value={item.selector.titleContains ?? ""}
              onChange={(_, data) => update("selector", { ...item.selector, titleContains: data.value || undefined })}
            />
          </Field>
          <Field label={t("inspector.windowClass")}>
            <Input
              value={item.selector.windowClass ?? ""}
              onChange={(_, data) => update("selector", { ...item.selector, windowClass: data.value || undefined })}
            />
          </Field>
        </section>
      </div>
      <footer className="app-inspector__footer">
        <Button appearance="subtle" icon={<Delete20Regular />} onClick={onRemove}>{t("common.remove")}</Button>
        <Button appearance="primary" onClick={onClose}>{t("common.done")}</Button>
      </footer>
    </aside>
  );
}
