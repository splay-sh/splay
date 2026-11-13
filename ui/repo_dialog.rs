//! The dialog for adding a repository.

use gtk::{
    Align, Box as GtkBox, Button, Entry, EventControllerKey, HeaderBar, Label, Orientation, Window,
    gdk, glib, pango::WrapMode, prelude::*,
};
use std::rc::Rc;

use crate::{
    app::{AppState, schedule_refresh},
    data::add_repository,
    exec,
};

/// Opens the dialog for adding a repository, over the main window.
pub(crate) fn open_add_repository_dialog(state: &Rc<AppState>) {
    let parent = state
        .detail_widgets
        .borrow()
        .as_ref()
        .and_then(|widgets| widgets.container.root())
        .and_downcast::<Window>();

    let dialog = Window::builder()
        .title("Add Repository")
        .modal(true)
        .resizable(false)
        .destroy_with_parent(true)
        .default_width(440)
        .build();
    dialog.set_transient_for(parent.as_ref());
    dialog.add_css_class("repo-dialog");

    let cancel = Button::with_label("Cancel");
    let add = Button::with_label("Add");
    add.add_css_class("suggested-action");
    add.set_sensitive(false);

    let header = HeaderBar::new();
    header.set_show_title_buttons(false);
    header.pack_start(&cancel);
    header.pack_end(&add);
    dialog.set_titlebar(Some(&header));

    let body = GtkBox::new(Orientation::Vertical, 6);
    body.add_css_class("repo-dialog-body");

    let repository_entry = Entry::new();
    repository_entry.set_placeholder_text(Some("github.com/owner/name"));
    repository_entry.add_css_class("repo-dialog-entry");

    let alias_entry = Entry::new();
    alias_entry.set_placeholder_text(Some("my-alias"));
    alias_entry.add_css_class("repo-dialog-entry");

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);
    error_label.set_wrap_mode(WrapMode::WordChar);
    error_label.set_selectable(true);
    error_label.add_css_class("repo-dialog-error");
    error_label.set_visible(false);

    body.append(&field_label("Repository"));
    body.append(&repository_entry);
    body.append(&field_hint(
        "As host/owner/name, or a git remote URL. It is cloned when added.",
    ));
    body.append(&field_label("Alias (optional)"));
    body.append(&alias_entry);
    body.append(&field_hint("Shown in place of the repository's name."));
    body.append(&error_label);
    dialog.set_child(Some(&body));

    {
        let add = add.clone();
        let error_label = error_label.clone();
        repository_entry.connect_changed(move |entry| {
            add.set_sensitive(!entry.text().trim().is_empty());
            error_label.set_visible(false);
        });
    }
    {
        let error_label = error_label.clone();
        alias_entry.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }

    let submit: Rc<dyn Fn()> = {
        let state = state.clone();
        let dialog = dialog.clone();
        let add = add.clone();
        let cancel = cancel.clone();
        let repository_entry = repository_entry.clone();
        let alias_entry = alias_entry.clone();
        let error_label = error_label.clone();
        Rc::new(move || {
            let repository = repository_entry.text().trim().to_string();
            if repository.is_empty() || !add.is_sensitive() {
                return;
            }
            let alias = Some(alias_entry.text().trim().to_string()).filter(|a| !a.is_empty());

            set_busy(&[&add, &cancel], &[&repository_entry, &alias_entry], true);
            add.set_label("Adding…");
            error_label.set_visible(false);

            let state = state.clone();
            let dialog = dialog.clone();
            let add = add.clone();
            let cancel = cancel.clone();
            let repository_entry = repository_entry.clone();
            let alias_entry = alias_entry.clone();
            let error_label = error_label.clone();
            exec::dispatch(
                add_repository(repository, alias),
                move |result| match result {
                    Ok(repo) => {
                        // No longer busy, so that the dialog may close.
                        cancel.set_sensitive(true);
                        dialog.close();
                        show_repository(&state, repo.canonical());
                    }
                    Err(err) => {
                        set_busy(&[&add, &cancel], &[&repository_entry, &alias_entry], false);
                        add.set_label("Add");
                        error_label.set_label(&err.to_string());
                        error_label.set_visible(true);
                        repository_entry.grab_focus();
                    }
                },
            );
        })
    };

    {
        let submit = submit.clone();
        add.connect_clicked(move |_| submit());
    }
    {
        let submit = submit.clone();
        repository_entry.connect_activate(move |_| submit());
    }
    alias_entry.connect_activate(move |_| submit());
    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }

    // Escape cancels, unless a repository is being added.
    let keys = EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let cancel = cancel.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape && cancel.is_sensitive() {
                dialog.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    dialog.add_controller(keys);
    // Closing while a repository is being added would hide its error.
    {
        let cancel = cancel.clone();
        dialog.connect_close_request(move |_| {
            if cancel.is_sensitive() {
                glib::Propagation::Proceed
            } else {
                glib::Propagation::Stop
            }
        });
    }

    dialog.present();
    repository_entry.grab_focus();
}

/// Switches to a newly added repository, which has no workspace selected.
fn show_repository(state: &Rc<AppState>, repo_canonical: String) {
    *state.current_repository.borrow_mut() = Some(repo_canonical);
    *state.settings_repository.borrow_mut() = None;
    *state.selected_workspace.borrow_mut() = None;
    *state.selected_session.borrow_mut() = None;
    *state.editing_workspace.borrow_mut() = None;
    schedule_refresh(state, None, None);
}

fn set_busy(buttons: &[&Button], entries: &[&Entry], busy: bool) {
    for button in buttons {
        button.set_sensitive(!busy);
    }
    for entry in entries {
        entry.set_sensitive(!busy);
    }
}

fn field_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_halign(Align::Start);
    label.add_css_class("repo-dialog-label");
    label
}

fn field_hint(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("repo-dialog-hint");
    label
}
