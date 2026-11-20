use crate::error::{ApiError, ApiResult};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

pub const PRESET_SCHEMA_VERSION: u32 = 1;

/// Central migration registry for imported preset documents. Version 0 was the internal
/// pre-alpha shape; version 1 is the first public export contract.
pub fn migrate_preset_value(mut value: Value) -> ApiResult<(Value, Vec<Notice>)> {
    let mut notices = Vec::new();
    let mut version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    if version > PRESET_SCHEMA_VERSION {
        return Err(ApiError::new(
            "unsupportedSchema",
            format!("Preset schema {version} is newer than this WorkSets version"),
        ));
    }

    while version < PRESET_SCHEMA_VERSION {
        value = match version {
            0 => migrate_v0_to_v1(value)?,
            _ => {
                return Err(ApiError::new(
                    "unsupportedSchema",
                    format!("Preset schema {version} has no registered migration"),
                ));
            }
        };
        notices.push(Notice::warning(
            "presetSchemaMigrated",
            format!("Preset schema {version} was migrated to {}", version + 1),
        ));
        version += 1;
    }
    Ok((value, notices))
}

fn migrate_v0_to_v1(mut value: Value) -> ApiResult<Value> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ApiError::new("invalidPreset", "Preset import must be a JSON object"))?;
    object.insert("schemaVersion".to_string(), Value::from(1));
    object
        .entry("desktopPolicy".to_string())
        .or_insert_with(|| Value::String("exactProtected".to_string()));
    object
        .entry("monitors".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    object
        .entry("items".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    object
        .entry("createdAt".to_string())
        .or_insert_with(|| Value::String(Utc::now().to_rfc3339()));
    object
        .entry("updatedAt".to_string())
        .or_insert_with(|| Value::String(Utc::now().to_rfc3339()));
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetV1 {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub desktop_policy: DesktopPolicy,
    #[serde(default)]
    pub desktops: Vec<DesktopSpec>,
    #[serde(default)]
    pub monitors: Vec<MonitorRef>,
    #[serde(default)]
    pub items: Vec<LaunchItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
}

impl PresetV1 {
    pub fn empty(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            schema_version: PRESET_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            created_at: now,
            updated_at: now,
            desktop_policy: DesktopPolicy::ExactProtected,
            desktops: vec![DesktopSpec {
                id: Uuid::new_v4(),
                name: "Desktop 1".to_string(),
                index: 0,
            }],
            monitors: Vec::new(),
            items: Vec::new(),
            hotkey: None,
        }
    }

    pub fn validate(&self) -> ApiResult<()> {
        if self.schema_version != PRESET_SCHEMA_VERSION {
            return Err(ApiError::new(
                "unsupportedSchema",
                format!(
                    "Preset schema {} is not supported; expected {}",
                    self.schema_version, PRESET_SCHEMA_VERSION
                ),
            ));
        }
        let trimmed_name = self.name.trim();
        if trimmed_name.is_empty() || trimmed_name.chars().count() > 80 {
            return Err(ApiError::new(
                "invalidPresetName",
                "Preset name must contain between 1 and 80 characters",
            ));
        }
        if self.desktops.is_empty() || self.desktops.len() > 32 {
            return Err(ApiError::new(
                "invalidDesktopCount",
                "A preset must contain between 1 and 32 desktops",
            ));
        }
        if self.items.len() > 256 {
            return Err(ApiError::new(
                "tooManyItems",
                "A preset can contain at most 256 launch items",
            ));
        }

        let desktop_ids: HashSet<_> = self.desktops.iter().map(|desktop| desktop.id).collect();
        if desktop_ids.len() != self.desktops.len() {
            return Err(ApiError::new(
                "duplicateDesktopId",
                "Desktop identifiers must be unique",
            ));
        }
        let expected_indices: Vec<u32> = (0..self.desktops.len() as u32).collect();
        let mut actual_indices: Vec<u32> =
            self.desktops.iter().map(|desktop| desktop.index).collect();
        actual_indices.sort_unstable();
        if actual_indices != expected_indices {
            return Err(ApiError::new(
                "invalidDesktopOrder",
                "Desktop indices must be consecutive and start at zero",
            ));
        }

        let mut item_ids = HashSet::new();
        for item in &self.items {
            if !item_ids.insert(item.id) {
                return Err(ApiError::new(
                    "duplicateItemId",
                    "Launch item identifiers must be unique",
                ));
            }
            if !desktop_ids.contains(&item.desktop_id) {
                return Err(ApiError::new(
                    "unknownDesktop",
                    format!("Item '{}' targets an unknown desktop", item.display_name),
                ));
            }
            item.validate()?;
        }
        Ok(())
    }

    pub fn summary(&self) -> PresetSummary {
        PresetSummary {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            updated_at: self.updated_at,
            desktop_policy: self.desktop_policy,
            desktop_count: self.desktops.len() as u32,
            monitor_count: self.monitors.len() as u32,
            item_count: self.items.iter().filter(|item| item.enabled).count() as u32,
            hotkey: self.hotkey.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSpec {
    pub id: Uuid,
    pub name: String,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopPolicy {
    #[default]
    ExactProtected,
    ManagedOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchItem {
    pub id: Uuid,
    pub display_name: String,
    pub desktop_id: Uuid,
    pub launch_target: LaunchTarget,
    #[serde(default)]
    pub selector: WindowSelector,
    pub placement: WindowPlacementSpec,
    #[serde(default)]
    pub lifecycle: LifecyclePolicy,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl LaunchItem {
    fn validate(&self) -> ApiResult<()> {
        if self.display_name.trim().is_empty() {
            return Err(ApiError::new(
                "invalidItemName",
                "Every launch item needs a display name",
            ));
        }
        match &self.launch_target {
            LaunchTarget::Executable { path, args, .. } => {
                if path.trim().is_empty() {
                    return Err(ApiError::new(
                        "invalidExecutable",
                        "Executable path cannot be empty",
                    ));
                }
                if args.len() > 128 || args.iter().any(|argument| argument.contains('\0')) {
                    return Err(ApiError::new(
                        "invalidArguments",
                        "Executable arguments are invalid",
                    ));
                }
            }
            LaunchTarget::PackagedApp { aumid, args } => {
                if aumid.trim().is_empty()
                    || aumid.contains('\0')
                    || args.iter().any(|argument| argument.contains('\0'))
                {
                    return Err(ApiError::new(
                        "invalidPackagedApp",
                        "Packaged app identifier or arguments are invalid",
                    ));
                }
            }
            LaunchTarget::File { path } => {
                if path.trim().is_empty() || path.contains('\0') {
                    return Err(ApiError::new("invalidFile", "File path is invalid"));
                }
            }
            LaunchTarget::Url { url } => {
                let parsed = url::Url::parse(url)
                    .map_err(|_| ApiError::new("invalidUrl", "URL is invalid"))?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    return Err(ApiError::new(
                        "invalidUrlScheme",
                        "Only HTTP and HTTPS URLs are supported",
                    ));
                }
            }
        }
        if self.placement.physical.width < 1
            || self.placement.physical.height < 1
            || !self.placement.normalized.is_finite()
        {
            return Err(ApiError::new(
                "invalidPlacement",
                "Window placement contains invalid dimensions",
            ));
        }
        if let Some(pattern) = &self.selector.title_pattern {
            regex::Regex::new(pattern).map_err(|error| {
                ApiError::new(
                    "invalidTitlePattern",
                    format!("Window title regular expression is invalid: {error}"),
                )
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LaunchTarget {
    Executable {
        path: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
    PackagedApp {
        aumid: String,
        #[serde(default)]
        args: Vec<String>,
    },
    File {
        path: String,
    },
    Url {
        url: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aumid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_pattern: Option<String>,
    #[serde(default)]
    pub instance_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacementSpec {
    pub monitor: MonitorRef,
    pub physical: RectI32,
    pub normalized: NormalizedRect,
    pub state: WindowState,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorRef {
    pub id: String,
    pub device_name: String,
    pub bounds: RectI32,
    pub work_area: RectI32,
    pub dpi_x: u32,
    pub dpi_y: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RectI32 {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedRect {
    pub fn is_finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LifecyclePolicy {
    #[default]
    Reuse,
    Restart,
    LeaveOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetSummary {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub desktop_policy: DesktopPolicy,
    pub desktop_count: u32,
    pub monitor_count: u32,
    pub item_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: String,
    pub theme: Theme,
    pub run_at_startup: bool,
    pub minimize_to_tray: bool,
    pub telemetry_opt_in: bool,
    pub check_for_updates: bool,
    pub onboarding_complete: bool,
    pub default_policy: DesktopPolicy,
    pub capture_hotkey: String,
    pub close_behavior: CloseBehavior,
    #[serde(default)]
    pub hotkeys: BTreeMap<Uuid, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "de".to_string(),
            theme: Theme::System,
            run_at_startup: false,
            minimize_to_tray: true,
            telemetry_opt_in: false,
            check_for_updates: true,
            onboarding_complete: false,
            default_policy: DesktopPolicy::ExactProtected,
            capture_hotkey: "Ctrl+Shift+W".to_string(),
            close_behavior: CloseBehavior::Tray,
            hotkeys: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> ApiResult<()> {
        if !matches!(self.language.as_str(), "de" | "en") {
            return Err(ApiError::new(
                "invalidLanguage",
                "Language must be either 'de' or 'en'",
            ));
        }
        let mut shortcuts = HashSet::new();
        if self.capture_hotkey.trim().is_empty() {
            return Err(ApiError::new(
                "invalidCaptureHotkey",
                "Capture shortcut cannot be empty",
            ));
        }
        shortcuts.insert(self.capture_hotkey.trim().to_ascii_lowercase());
        for shortcut in self.hotkeys.values() {
            let normalized = shortcut.trim().to_ascii_lowercase();
            if normalized.is_empty() || !shortcuts.insert(normalized) {
                return Err(ApiError::new(
                    "hotkeyConflict",
                    "Preset shortcuts must be non-empty and unique",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    Tray,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub code: String,
    pub message: String,
    pub severity: NoticeSeverity,
}

impl Notice {
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: NoticeSeverity::Warning,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: NoticeSeverity::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NoticeSeverity {
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_preset_is_valid() {
        PresetV1::empty("Focus").validate().unwrap();
    }

    #[test]
    fn rejects_non_consecutive_desktops() {
        let mut preset = PresetV1::empty("Focus");
        preset.desktops[0].index = 2;
        assert_eq!(preset.validate().unwrap_err().code, "invalidDesktopOrder");
    }

    #[test]
    fn rejects_unsafe_url_scheme() {
        let mut preset = PresetV1::empty("Focus");
        let desktop_id = preset.desktops[0].id;
        preset.items.push(LaunchItem {
            id: Uuid::new_v4(),
            display_name: "Bad URL".to_string(),
            desktop_id,
            launch_target: LaunchTarget::Url {
                url: "file:///C:/secret.txt".to_string(),
            },
            selector: WindowSelector::default(),
            placement: WindowPlacementSpec {
                monitor: MonitorRef {
                    id: "DISPLAY1".to_string(),
                    device_name: "DISPLAY1".to_string(),
                    bounds: RectI32 {
                        width: 1920,
                        height: 1080,
                        ..RectI32::default()
                    },
                    work_area: RectI32 {
                        width: 1920,
                        height: 1040,
                        ..RectI32::default()
                    },
                    dpi_x: 96,
                    dpi_y: 96,
                    primary: true,
                },
                physical: RectI32 {
                    width: 800,
                    height: 600,
                    ..RectI32::default()
                },
                normalized: NormalizedRect {
                    width: 0.5,
                    height: 0.5,
                    ..NormalizedRect::default()
                },
                state: WindowState::Normal,
            },
            lifecycle: LifecyclePolicy::Reuse,
            enabled: true,
        });
        assert_eq!(preset.validate().unwrap_err().code, "invalidUrlScheme");
    }

    #[test]
    fn settings_reject_duplicate_shortcuts() {
        let mut settings = Settings::default();
        settings
            .hotkeys
            .insert(Uuid::new_v4(), "Ctrl+Alt+1".to_string());
        settings
            .hotkeys
            .insert(Uuid::new_v4(), "ctrl+alt+1".to_string());
        assert_eq!(settings.validate().unwrap_err().code, "hotkeyConflict");
    }

    #[test]
    fn migration_registry_passes_v1_through() {
        let preset = PresetV1::empty("Focus");
        let value = serde_json::to_value(&preset).unwrap();
        let (migrated, notices) = migrate_preset_value(value.clone()).unwrap();
        assert_eq!(migrated, value);
        assert!(notices.is_empty());
    }

    #[test]
    fn migration_registry_upgrades_internal_v0() {
        let preset = PresetV1::empty("Focus");
        let mut value = serde_json::to_value(&preset).unwrap();
        value.as_object_mut().unwrap().remove("schemaVersion");
        let (migrated, notices) = migrate_preset_value(value).unwrap();
        assert_eq!(migrated["schemaVersion"], 1);
        assert_eq!(notices.len(), 1);
        let migrated: PresetV1 = serde_json::from_value(migrated).unwrap();
        migrated.validate().unwrap();
    }

    #[test]
    fn migration_registry_rejects_future_schema() {
        let preset = PresetV1::empty("Focus");
        let mut value = serde_json::to_value(&preset).unwrap();
        value["schemaVersion"] = Value::from(PRESET_SCHEMA_VERSION + 1);
        assert_eq!(
            migrate_preset_value(value).unwrap_err().code,
            "unsupportedSchema"
        );
    }
}
