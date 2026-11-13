//! The window's title bar: a repository switcher, what is shown, and the
//! terminal tabs of the selected workspace. The sidebar lists the workspaces
//! of the repository picked here.

use gtk::{
    Align, Box as GtkBox, Button, Image, Label, MenuButton, Orientation, PackType, Popover,
    PositionType, Separator, ToggleButton, WindowControls, WindowHandle, prelude::*,
};
use std::rc::Rc;

use crate::{
    app::{
        AppState, find_workspace_by_ref, preferred_session_for_workspace,
        schedule_render_current_ui, workspace_ref,
    },
    data::{WorkspaceEntry, WorkspaceGroup},
    repo_dialog::open_add_repository_dialog,
    workspace_panel::DetailWidgets,
};

#[derive(Clone)]
pub struct TitlebarWidgets {
    pub handle: WindowHandle,
    repo_label: Label,
    repo_popover: Popover,
    repo_list: GtkBox,
}

impl TitlebarWidgets {
    /// Builds the title bar around the detail panel's terminal toolbar.
    pub fn new(detail: &DetailWidgets, sidebar_host: &GtkBox) -> Self {
        let bar = GtkBox::new(Orientation::Horizontal, 8);
        bar.add_css_class("app-titlebar");

        let start_controls = WindowControls::new(PackType::Start);
        start_controls.set_valign(Align::Center);
        let end_controls = WindowControls::new(PackType::End);
        end_controls.set_valign(Align::Center);

        let repo_label = Label::new(Some("Splay"));
        let repo_list = GtkBox::new(Orientation::Vertical, 2);
        repo_list.set_size_request(220, -1);
        let repo_popover = Popover::new();
        repo_popover.set_position(PositionType::Bottom);
        repo_popover.set_has_arrow(false);
        repo_popover.add_css_class("repo-switcher-popover");
        repo_popover.set_child(Some(&repo_list));
        let repo_button = build_repo_switcher(&repo_label, &repo_popover);

        detail.context_label.set_valign(Align::Center);
        detail.session_toolbar.set_valign(Align::Center);

        bar.append(&start_controls);
        bar.append(&build_sidebar_toggle(sidebar_host));
        bar.append(&repo_button);
        bar.append(&detail.context_label);
        bar.append(&detail.session_toolbar);
        bar.append(&end_controls);

        let handle = WindowHandle::new();
        handle.set_child(Some(&bar));

        Self {
            handle,
            repo_label,
            repo_popover,
            repo_list,
        }
    }
}

/// Updates the repository switcher for the current repository.
pub(crate) fn render_repo_switcher(
    state: &Rc<AppState>,
    groups: &[WorkspaceGroup],
    current: Option<&str>,
) {
    let Some(widgets) = state.titlebar_widgets.borrow().clone() else {
        return;
    };

    let group = current.and_then(|repo| groups.iter().find(|group| group.repo_canonical == repo));
    widgets.repo_label.set_label(match group {
        Some(group) => &group.repo_label,
        None if state.loaded.get() => "No repositories",
        None => "Splay",
    });

    while let Some(child) = widgets.repo_list.first_child() {
        widgets.repo_list.remove(&child);
    }
    let mut sorted = groups.iter().collect::<Vec<_>>();
    sorted.sort_by_cached_key(|group| group.repo_label.to_lowercase());
    for group in sorted {
        let title = Label::new(Some(&group.repo_label));
        title.set_halign(Align::Start);

        let item = Button::new();
        item.set_child(Some(&title));
        item.add_css_class("repo-switcher-item");
        if current == Some(group.repo_canonical.as_str()) {
            item.add_css_class("repo-switcher-item-active");
        }
        let state = state.clone();
        let popover = widgets.repo_popover.downgrade();
        let repo_canonical = group.repo_canonical.clone();
        item.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            switch_repository(&state, &repo_canonical);
        });
        widgets.repo_list.append(&item);
    }

    if !groups.is_empty() {
        let separator = Separator::new(Orientation::Horizontal);
        separator.add_css_class("repo-switcher-separator");
        widgets.repo_list.append(&separator);
    }
    let title = Label::new(Some("Add repository…"));
    title.set_halign(Align::Start);
    let item = Button::new();
    item.set_child(Some(&title));
    item.add_css_class("repo-switcher-item");
    let state = state.clone();
    let popover = widgets.repo_popover.downgrade();
    item.connect_clicked(move |_| {
        if let Some(popover) = popover.upgrade() {
            popover.popdown();
        }
        open_add_repository_dialog(&state);
    });
    widgets.repo_list.append(&item);
}

/// The repository whose workspaces the sidebar lists: that of the selected
/// workspace or settings page, else the one last switched to, else the first.
pub(crate) fn current_repository(
    state: &Rc<AppState>,
    groups: &[WorkspaceGroup],
    selected: Option<&WorkspaceEntry>,
) -> Option<String> {
    let has_repo = |repo: &str| groups.iter().any(|group| group.repo_canonical == repo);
    let repo = selected
        .map(|workspace| workspace.repo_canonical.clone())
        .or_else(|| state.settings_repository.borrow().clone())
        .filter(|repo| has_repo(repo))
        .or_else(|| {
            state
                .current_repository
                .borrow()
                .clone()
                .filter(|repo| has_repo(repo))
        })
        .or_else(|| groups.first().map(|group| group.repo_canonical.clone()))?;
    *state.current_repository.borrow_mut() = Some(repo.clone());
    if let Some(workspace) = selected {
        state
            .repository_workspaces
            .borrow_mut()
            .insert(repo.clone(), workspace_ref(workspace));
    }
    Some(repo)
}

/// Switches to a repository, showing the workspace last shown in it.
fn switch_repository(state: &Rc<AppState>, repo_canonical: &str) {
    *state.current_repository.borrow_mut() = Some(repo_canonical.to_string());
    *state.settings_repository.borrow_mut() = None;
    *state.editing_workspace.borrow_mut() = None;

    let groups = state.workspace_groups.borrow().clone();
    let remembered = state
        .repository_workspaces
        .borrow()
        .get(repo_canonical)
        .cloned()
        .filter(|workspace| find_workspace_by_ref(&groups, workspace).is_some());
    match remembered {
        Some(workspace) => {
            let session = preferred_session_for_workspace(state, &workspace, None);
            *state.selected_workspace.borrow_mut() = Some(workspace.clone());
            *state.selected_session.borrow_mut() = session.clone();
            schedule_render_current_ui(state, Some(workspace), session);
        }
        None => {
            *state.selected_workspace.borrow_mut() = None;
            *state.selected_session.borrow_mut() = None;
            schedule_render_current_ui(state, None, None);
        }
    }
}

fn build_repo_switcher(repo_label: &Label, popover: &Popover) -> MenuButton {
    let icon = Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(12);
    icon.add_css_class("repo-switcher-icon");

    let arrow = Image::from_icon_name("pan-down-symbolic");
    arrow.set_pixel_size(12);
    arrow.add_css_class("repo-switcher-arrow");

    let content = GtkBox::new(Orientation::Horizontal, 6);
    content.append(&icon);
    content.append(repo_label);
    content.append(&arrow);

    let button = MenuButton::new();
    button.set_child(Some(&content));
    button.set_popover(Some(popover));
    button.set_valign(Align::Center);
    button.set_focus_on_click(false);
    button.set_tooltip_text(Some("Switch repository"));
    button.add_css_class("repo-switcher");
    button
}

/// The sidebar is shown by default; hiding it gives the terminal its width.
fn build_sidebar_toggle(sidebar_host: &GtkBox) -> ToggleButton {
    let toggle = ToggleButton::new();
    toggle.set_icon_name("sidebar-show-symbolic");
    toggle.set_valign(Align::Center);
    toggle.set_focus_on_click(false);
    toggle.set_active(true);
    toggle.set_tooltip_text(Some("Hide sidebar"));
    toggle.add_css_class("titlebar-action");

    let sidebar_host = sidebar_host.clone();
    toggle.connect_toggled(move |toggle| {
        sidebar_host.set_visible(toggle.is_active());
        toggle.set_tooltip_text(Some(if toggle.is_active() {
            "Hide sidebar"
        } else {
            "Show sidebar"
        }));
    });
    toggle
}
