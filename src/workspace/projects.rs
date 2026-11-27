//! Local-first development project tracker, stored with the user's other workspace data.

use crate::clock::now_ms;
use crate::lock::lock;
use crate::model::watchlist::new_id;
use crate::store::SharedStore;
use eframe::egui;
use serde::{Deserialize, Serialize};

const KEY: &str = "gk.projects.v1";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
enum Status {
    Idea,
    #[default]
    Planned,
    InProgress,
    Testing,
    Done,
    Blocked,
}
impl Status {
    const ALL: [Self; 6] = [
        Self::Idea,
        Self::Planned,
        Self::InProgress,
        Self::Testing,
        Self::Done,
        Self::Blocked,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Idea => "Idea",
            Self::Planned => "Planned",
            Self::InProgress => "In Progress",
            Self::Testing => "Testing",
            Self::Done => "Done",
            Self::Blocked => "Blocked",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
enum Priority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}
impl Priority {
    const ALL: [Self; 4] = [Self::Low, Self::Normal, Self::High, Self::Critical];
    fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Task {
    id: u64,
    title: String,
    description: String,
    status: Status,
    priority: Priority,
    branch: String,
    parent: Option<u64>,
    created_ms: i64,
    updated_ms: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Project {
    id: u64,
    name: String,
    description: String,
    branch: String,
    status: Status,
    created_ms: i64,
    updated_ms: i64,
    tasks: Vec<Task>,
}
#[derive(Default, Deserialize, Serialize)]
struct Projects {
    projects: Vec<Project>,
}

#[derive(Default)]
pub(super) struct ProjectsPane {
    visible: bool,
    loaded: bool,
    data: Projects,
    selected: Option<u64>,
    new_project: String,
    new_task: String,
    notice: Option<String>,
}
impl ProjectsPane {
    pub(super) fn toggle(&mut self) {
        self.visible = !self.visible;
    }
    pub(super) fn visible(&self) -> bool {
        self.visible
    }
}

pub(super) fn show(ui: &mut egui::Ui, pane: &mut ProjectsPane, store: &SharedStore) {
    if !pane.loaded {
        pane.loaded = true;
        pane.data = lock(store).setting(KEY).unwrap_or_default();
        if pane.data.projects.is_empty() {
            pane.data = starter_projects();
            if let Err(error) = lock(store).set_setting(KEY, &pane.data) {
                pane.notice = Some(error.to_string());
            }
        }
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.heading("Projects");
        ui.label("Local development tracker");
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut pane.new_project);
        if ui.button("New project").clicked() && !pane.new_project.trim().is_empty() {
            let now = now_ms();
            let project = Project {
                id: new_id(),
                name: pane.new_project.trim().to_owned(),
                description: String::new(),
                branch: String::new(),
                status: Status::Planned,
                created_ms: now,
                updated_ms: now,
                tasks: Vec::new(),
            };
            pane.selected = Some(project.id);
            pane.data.projects.push(project);
            pane.new_project.clear();
            changed = true;
        }
    });
    ui.separator();
    ui.columns(2, |columns| {
        egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
            for project in &pane.data.projects {
                let selected = pane.selected == Some(project.id);
                if ui
                    .selectable_label(
                        selected,
                        format!("{} · {}", project.name, project.status.label()),
                    )
                    .clicked()
                {
                    pane.selected = Some(project.id);
                }
            }
        });
        let Some(id) = pane.selected else {
            columns[1].label("Create or select a project.");
            return;
        };
        let Some(project) = pane
            .data
            .projects
            .iter_mut()
            .find(|project| project.id == id)
        else {
            return;
        };
        columns[1].heading(&project.name);
        changed |= columns[1].text_edit_singleline(&mut project.name).changed();
        columns[1].label("Description");
        changed |= columns[1]
            .add(egui::TextEdit::multiline(&mut project.description).desired_rows(3))
            .changed();
        columns[1].horizontal(|ui| {
            ui.label("Branch");
            changed |= ui.text_edit_singleline(&mut project.branch).changed();
            egui::ComboBox::from_id_salt(("project-status", id))
                .selected_text(project.status.label())
                .show_ui(ui, |ui| {
                    for status in Status::ALL {
                        changed |= ui
                            .selectable_value(&mut project.status, status, status.label())
                            .changed();
                    }
                });
        });
        columns[1].separator();
        columns[1].heading("Tasks");
        columns[1].horizontal(|ui| {
            ui.text_edit_singleline(&mut pane.new_task);
            if ui.button("Add task").clicked() && !pane.new_task.trim().is_empty() {
                let now = now_ms();
                project.tasks.push(Task {
                    id: new_id(),
                    title: pane.new_task.trim().to_owned(),
                    description: String::new(),
                    status: Status::Planned,
                    priority: Priority::Normal,
                    branch: project.branch.clone(),
                    parent: None,
                    created_ms: now,
                    updated_ms: now,
                });
                pane.new_task.clear();
                changed = true;
            }
        });
        let mut remove = None;
        for (index, task) in project.tasks.iter_mut().enumerate() {
            columns[1].group(|ui| {
                ui.horizontal(|ui| {
                    let mut done = task.status == Status::Done;
                    if ui.checkbox(&mut done, "").changed() {
                        task.status = if done { Status::Done } else { Status::Planned };
                        changed = true;
                    }
                    changed |= ui.text_edit_singleline(&mut task.title).changed();
                    egui::ComboBox::from_id_salt(("task-status", task.id))
                        .selected_text(task.status.label())
                        .show_ui(ui, |ui| {
                            for status in Status::ALL {
                                changed |= ui
                                    .selectable_value(&mut task.status, status, status.label())
                                    .changed();
                            }
                        });
                    egui::ComboBox::from_id_salt(("task-priority", task.id))
                        .selected_text(task.priority.label())
                        .show_ui(ui, |ui| {
                            for priority in Priority::ALL {
                                changed |= ui
                                    .selectable_value(
                                        &mut task.priority,
                                        priority,
                                        priority.label(),
                                    )
                                    .changed();
                            }
                        });
                    if ui.small_button("×").clicked() {
                        remove = Some(index);
                    }
                });
                changed |= ui.text_edit_singleline(&mut task.branch).changed();
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut task.description)
                            .hint_text("Task notes")
                            .desired_rows(2),
                    )
                    .changed();
            });
        }
        if let Some(index) = remove {
            project.tasks.remove(index);
            changed = true;
        }
        if changed {
            project.updated_ms = now_ms();
        }
    });
    if changed {
        if let Err(error) = lock(store).set_setting(KEY, &pane.data) {
            pane.notice = Some(error.to_string());
        }
    }
    if let Some(notice) = &pane.notice {
        ui.colored_label(egui::Color32::LIGHT_RED, notice);
    }
}

fn starter_projects() -> Projects {
    let now = now_ms();
    let task = |title: &str, description: &str, branch: &str| Task {
        id: new_id(),
        title: title.to_owned(),
        description: description.to_owned(),
        status: Status::Planned,
        priority: Priority::High,
        branch: branch.to_owned(),
        parent: None,
        created_ms: now,
        updated_ms: now,
    };
    Projects { projects: vec![
        Project {
            id: new_id(), name: "DegenView V1".to_owned(),
            description: "Native market dashboard fork: data, charting, Pine, screeners and broker workflow.".to_owned(),
            branch: "main".to_owned(), status: Status::InProgress, created_ms: now, updated_ms: now,
            tasks: vec![
                task("Define single-word concepts and keywords", "Create the controlled vocabulary for app modules, data sources, chart tools, screeners, layouts and broker workflows.", "gk/v3-projects"),
                task("Verify Local NSE screener", "Build and test saved queries, turnover template and saving matched symbols as a watchlist.", "main"),
                task("Resolve upstream upgrade", "Finish integration and verification of gk/v2-upstream-update before merging it into main.", "gk/v2-upstream-update"),
            ],
        },
        Project {
            id: new_id(), name: "Algo Trading Price Action".to_owned(),
            description: "Research and implementation system for price action, market structure, ranges, boundaries and trade validation.".to_owned(),
            branch: "main".to_owned(), status: Status::InProgress, created_ms: now, updated_ms: now,
            tasks: vec![
                task("Define single-word concepts and keywords", "Define controlled names for pivots, legs, cycles, ranges, boundaries, gaps, acceptance, rejection, entries, exits and targets.", "main"),
                task("Document range and boundary rules", "Turn the accepted market-structure and range rules into testable specifications.", "main"),
                task("Validate price-action signals", "Use the signal validator to measure historical condition counts and outcomes.", "main"),
            ],
        },
    ] }
}
