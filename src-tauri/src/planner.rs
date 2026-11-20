use crate::domain::{DesktopPolicy, Notice, PresetV1};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPlan {
    pub preset_id: Uuid,
    pub target_desktop_count: u32,
    pub current_desktop_count: u32,
    pub requires_confirmation: bool,
    pub actions: Vec<PlannedAction>,
    pub foreign_windows: Vec<WindowReference>,
    pub warnings: Vec<Notice>,
}

impl ApplyPlan {
    pub fn build(
        preset: &PresetV1,
        current_desktop_count: u32,
        foreign_windows: Vec<WindowReference>,
        matched_item_ids: &[Uuid],
    ) -> Self {
        let target_desktop_count = preset.desktops.len() as u32;
        let mut actions = Vec::new();
        let mut warnings = Vec::new();

        if current_desktop_count < target_desktop_count {
            actions.push(PlannedAction::CreateDesktops {
                count: target_desktop_count - current_desktop_count,
            });
        } else if current_desktop_count > target_desktop_count
            && preset.desktop_policy == DesktopPolicy::ExactProtected
        {
            actions.push(PlannedAction::RemoveDesktops {
                count: current_desktop_count - target_desktop_count,
            });
        }

        for item in preset.items.iter().filter(|item| item.enabled) {
            if item.lifecycle == crate::domain::LifecyclePolicy::LeaveOpen {
                if matched_item_ids.contains(&item.id) {
                    // Report the existing window, but do not move, place, launch, or close it.
                    actions.push(PlannedAction::ReuseWindow { item_id: item.id });
                } else {
                    warnings.push(Notice::warning(
                        "leaveOpenWindowMissing",
                        format!(
                            "'{}' is not open and will not be launched",
                            item.display_name
                        ),
                    ));
                }
                continue;
            }
            if item.lifecycle == crate::domain::LifecyclePolicy::Restart {
                actions.push(PlannedAction::LaunchItem { item_id: item.id });
            } else if matched_item_ids.contains(&item.id) {
                actions.push(PlannedAction::ReuseWindow { item_id: item.id });
            } else {
                actions.push(PlannedAction::LaunchItem { item_id: item.id });
            }
            actions.push(PlannedAction::PlaceWindow { item_id: item.id });
        }

        let requires_confirmation = preset.desktop_policy == DesktopPolicy::ExactProtected
            && current_desktop_count > target_desktop_count
            && !foreign_windows.is_empty();
        if requires_confirmation {
            warnings.push(Notice::warning(
                "foreignWindowsOnRemovedDesktops",
                "Unmanaged windows must be moved before extra desktops can be removed",
            ));
        }

        Self {
            preset_id: preset.id,
            target_desktop_count,
            current_desktop_count,
            requires_confirmation,
            actions,
            foreign_windows,
            warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlannedAction {
    CreateDesktops { count: u32 },
    RemoveDesktops { count: u32 },
    ReuseWindow { item_id: Uuid },
    LaunchItem { item_id: Uuid },
    PlaceWindow { item_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowReference {
    pub native_id: String,
    pub title: String,
    pub application: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOptions {
    #[serde(default)]
    pub confirm_desktop_changes: bool,
    #[serde(default)]
    pub force_close_managed: bool,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_ms: u64,
}

const fn default_startup_timeout() -> u64 {
    10_000
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            confirm_desktop_changes: false,
            force_close_managed: false,
            startup_timeout_ms: default_startup_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub id: Uuid,
    pub preset_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub status: ApplyStatus,
    pub steps: Vec<ApplyStepResult>,
    pub items: Vec<ItemApplyResult>,
    pub notices: Vec<Notice>,
    pub launched_count: u32,
    pub reused_count: u32,
    pub closed_count: u32,
    pub adjusted_count: u32,
}

impl ApplyReport {
    pub fn new(preset_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            preset_id,
            started_at: now,
            finished_at: now,
            status: ApplyStatus::Success,
            steps: Vec::new(),
            items: Vec::new(),
            notices: Vec::new(),
            launched_count: 0,
            reused_count: 0,
            closed_count: 0,
            adjusted_count: 0,
        }
    }

    pub fn finish(&mut self) {
        self.finished_at = Utc::now();
        let has_errors = self
            .notices
            .iter()
            .any(|notice| notice.severity == crate::domain::NoticeSeverity::Error)
            || self
                .items
                .iter()
                .any(|item| item.status == ItemApplyStatus::Failed);
        let has_warnings = self
            .notices
            .iter()
            .any(|notice| notice.severity == crate::domain::NoticeSeverity::Warning)
            || self.items.iter().any(|item| {
                matches!(
                    item.status,
                    ItemApplyStatus::Adjusted | ItemApplyStatus::Skipped
                )
            });
        if self.status != ApplyStatus::Cancelled && self.status != ApplyStatus::NeedsAttention {
            self.status = if has_errors && self.items.is_empty() {
                ApplyStatus::Failed
            } else if has_errors || has_warnings {
                ApplyStatus::Partial
            } else {
                ApplyStatus::Success
            };
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApplyStatus {
    Success,
    Partial,
    NeedsAttention,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyStepResult {
    pub step: ApplyStep,
    pub status: StepStatus,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApplyStep {
    Preflight,
    Desktops,
    Reuse,
    Launch,
    Placement,
    Verify,
    CloseObsolete,
    Activate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Success,
    Skipped,
    Warning,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemApplyResult {
    pub item_id: Uuid,
    pub display_name: String,
    pub status: ItemApplyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ItemApplyStatus {
    Launched,
    Reused,
    Adjusted,
    Skipped,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applying_matching_empty_preset_is_idempotent() {
        let preset = PresetV1::empty("Focus");
        let plan = ApplyPlan::build(&preset, 1, Vec::new(), &[]);
        assert!(plan.actions.is_empty());
        assert!(!plan.requires_confirmation);
        assert_eq!(
            serde_json::to_value(ApplyStep::CloseObsolete).unwrap(),
            serde_json::json!("closeObsolete")
        );
    }

    #[test]
    fn exact_policy_protects_foreign_windows() {
        let preset = PresetV1::empty("Focus");
        let foreign = vec![WindowReference {
            native_id: "123".to_string(),
            title: "Draft".to_string(),
            application: "Editor".to_string(),
            desktop_index: Some(2),
        }];
        let plan = ApplyPlan::build(&preset, 3, foreign, &[]);
        assert!(plan.requires_confirmation);
    }

    #[test]
    fn exact_policy_removes_empty_extra_desktops_without_confirmation() {
        let preset = PresetV1::empty("Focus");
        let plan = ApplyPlan::build(&preset, 3, Vec::new(), &[]);
        assert!(!plan.requires_confirmation);
        assert!(plan
            .actions
            .iter()
            .any(|action| matches!(action, PlannedAction::RemoveDesktops { count: 2 })));
    }

    #[test]
    fn managed_only_never_removes_foreign_desktops() {
        let mut preset = PresetV1::empty("Focus");
        preset.desktop_policy = DesktopPolicy::ManagedOnly;
        let plan = ApplyPlan::build(&preset, 4, Vec::new(), &[]);
        assert!(!plan
            .actions
            .iter()
            .any(|action| matches!(action, PlannedAction::RemoveDesktops { .. })));
    }

    #[test]
    fn matched_leave_open_window_is_never_placed() {
        use crate::domain::{
            LaunchItem, LaunchTarget, LifecyclePolicy, MonitorRef, NormalizedRect, RectI32,
            WindowPlacementSpec, WindowSelector, WindowState,
        };

        let mut preset = PresetV1::empty("Keep");
        let item_id = Uuid::new_v4();
        preset.items.push(LaunchItem {
            id: item_id,
            display_name: "Draft".to_string(),
            desktop_id: preset.desktops[0].id,
            launch_target: LaunchTarget::Executable {
                path: r"C:\Tools\editor.exe".to_string(),
                args: Vec::new(),
                working_directory: None,
            },
            selector: WindowSelector {
                executable_path: Some(r"C:\Tools\editor.exe".to_string()),
                ..WindowSelector::default()
            },
            placement: WindowPlacementSpec {
                monitor: MonitorRef {
                    id: "monitor".to_string(),
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
            lifecycle: LifecyclePolicy::LeaveOpen,
            enabled: true,
        });
        let plan = ApplyPlan::build(&preset, 1, Vec::new(), &[item_id]);
        assert_eq!(plan.actions, vec![PlannedAction::ReuseWindow { item_id }]);
        assert!(!plan
            .actions
            .iter()
            .any(|action| matches!(action, PlannedAction::PlaceWindow { .. })));
    }
}
