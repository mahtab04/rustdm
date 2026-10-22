use crate::{
    backend::{Backend, Event},
    model::{DownloadForm, Filter, bytes},
    theme::*,
};
use eframe::egui::{self, Align, Button, Frame, Layout, RichText, Stroke, Ui, Vec2};
use rustdm_core::{HistoryEntry, JobState, QueueJob, TransferMetrics};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) struct DesktopApp {
    backend: Backend,
    jobs: Vec<QueueJob>,
    history: Vec<HistoryEntry>,
    metrics: HashMap<u64, TransferMetrics>,
    filter: Filter,
    search: String,
    form: DownloadForm,
    show_add: bool,
    focus_url: bool,
    adding: bool,
    picking: bool,
    notice: Option<String>,
    closing: bool,
    refreshed: Instant,
}

impl DesktopApp {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        directory: PathBuf,
    ) -> anyhow::Result<Self> {
        apply(&cc.egui_ctx);
        let backend = Backend::new(directory, cc.egui_ctx.clone())?;
        backend.refresh();
        Ok(Self {
            backend,
            jobs: Vec::new(),
            history: Vec::new(),
            metrics: HashMap::new(),
            filter: Filter::All,
            search: String::new(),
            form: DownloadForm::default(),
            show_add: false,
            focus_url: false,
            adding: false,
            picking: false,
            notice: None,
            closing: false,
            refreshed: Instant::now(),
        })
    }

    fn poll(&mut self) {
        while let Ok(event) = self.backend.events.try_recv() {
            match event {
                Event::Snapshot(Ok(snapshot)) => {
                    self.jobs = snapshot.jobs;
                    self.history = snapshot.history;
                }
                Event::Snapshot(Err(error)) => self.notice = Some(error),
                Event::Changed => self.backend.refresh(),
                Event::Added(result) => {
                    self.adding = false;
                    match result {
                        Ok(start) => {
                            self.show_add = false;
                            self.form = DownloadForm::default();
                            self.backend.refresh();
                            if start {
                                self.backend.start();
                            }
                        }
                        Err(error) => self.form.error = Some(error),
                    }
                }
                Event::Retried(result) => {
                    if let Err(error) = result {
                        self.notice = Some(error);
                    }
                    self.backend.refresh();
                }
                Event::Stopped(result) => {
                    self.notice = Some(match result {
                        Ok(summary) if summary.cancelled => {
                            "Queue paused. Your partial downloads are saved.".into()
                        }
                        Ok(summary) => format!(
                            "Queue finished: {} completed, {} failed.",
                            summary.completed, summary.failed
                        ),
                        Err(error) => error,
                    });
                    self.backend.refresh();
                }
                Event::Destination(path) => {
                    self.picking = false;
                    if let Some(path) = path {
                        self.form.output = path.display().to_string();
                    }
                }
            }
        }
        if let Some((id, metrics)) = *self.backend.progress.borrow_and_update() {
            self.metrics.insert(id, metrics);
        }
        if self.refreshed.elapsed() >= Duration::from_secs(2) {
            self.backend.refresh();
            self.refreshed = Instant::now();
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            download_icon(ui, 34.0);
            ui.label(RichText::new("RustDM").size(24.0).strong());
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new("Your downloads. In control.")
                .small()
                .color(MUTED),
        );
        ui.add_space(40.0);
        ui.label(RichText::new("WORKSPACE").small().color(MUTED));
        ui.add_space(10.0);
        for (filter, label, count) in [
            (Filter::All, "All downloads", self.jobs.len()),
            (
                Filter::Active,
                "In progress",
                self.jobs
                    .iter()
                    .filter(|job| Filter::Active.accepts(job.state()))
                    .count(),
            ),
            (
                Filter::Completed,
                "Completed",
                self.jobs
                    .iter()
                    .filter(|job| job.state() == JobState::Completed)
                    .count(),
            ),
            (Filter::History, "History", self.history.len()),
        ] {
            let selected = self.filter == filter;
            let text = format!("{label}    {count}");
            let button =
                Button::new(RichText::new(text).color(if selected { TEXT } else { MUTED }))
                    .fill(if selected {
                        BLUE.gamma_multiply(0.2)
                    } else {
                        SIDEBAR
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(8);
            if ui.add_sized([ui.available_width(), 42.0], button).clicked() {
                self.filter = filter;
            }
        }
        ui.add_space(32.0);
        ui.separator();
        ui.add_space(16.0);
        ui.label(RichText::new("DESKTOP PREVIEW").small().color(BLUE));
        ui.label(
            RichText::new("One engine. Every download.")
                .small()
                .color(MUTED),
        );
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(MUTED),
            );
            ui.label(RichText::new("Queue storage").small().color(MUTED))
                .on_hover_text(self.backend.directory().display().to_string());
            ui.add_space(12.0);
        });
    }

    fn dashboard(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(if self.filter == Filter::History {
                    "Download history"
                } else {
                    "Downloads"
                });
                ui.label(
                    RichText::new("Everything you need, right where you left it.").color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        Button::new(RichText::new("+  New download").strong().color(TEXT))
                            .fill(BLUE)
                            .corner_radius(8),
                    )
                    .clicked()
                {
                    self.show_add = true;
                    self.focus_url = true;
                }
            });
        });
        ui.add_space(24.0);
        let running = self
            .jobs
            .iter()
            .filter(|job| job.state() == JobState::Running)
            .count();
        let completed = self
            .jobs
            .iter()
            .filter(|job| job.state() == JobState::Completed)
            .count();
        let speed = if self.backend.running() {
            self.metrics
                .iter()
                .find(|(id, _)| {
                    self.jobs
                        .iter()
                        .any(|job| job.id() == **id && job.state() == JobState::Running)
                })
                .and_then(|(_, value)| value.bytes_per_second)
                .map(|value| format!("{}/s", bytes(value as u64)))
                .unwrap_or_else(|| "Measuring...".into())
        } else {
            "0 B/s".into()
        };
        ui.columns(3, |columns| {
            stat(
                &mut columns[0],
                "ACTIVE TRANSFERS",
                &running.to_string(),
                BLUE,
            );
            stat(&mut columns[1], "DOWNLOAD SPEED", &speed, GREEN);
            stat(&mut columns[2], "COMPLETED", &completed.to_string(), TEXT);
        });
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search by filename or destination...")
                    .desired_width(300.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.backend.running() {
                    if ui.button("Pause queue").clicked() {
                        self.backend.pause();
                    }
                } else if ui
                    .add_enabled(
                        self.jobs
                            .iter()
                            .any(|job| Filter::Active.accepts(job.state())),
                        Button::new("Start queue"),
                    )
                    .clicked()
                {
                    self.backend.start();
                }
                if ui.small_button("Refresh").clicked() {
                    self.backend.refresh();
                }
            });
        });
        if let Some(notice) = self.notice.clone() {
            ui.add_space(8.0);
            Frame::new()
                .fill(SURFACE)
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(notice).small().color(MUTED));
                        if ui.small_button("Dismiss").clicked() {
                            self.notice = None;
                        }
                    });
                });
        }
        ui.add_space(16.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let search = self.search.to_lowercase();
                if self.filter == Filter::History {
                    let entries: Vec<_> = self
                        .history
                        .iter()
                        .rev()
                        .filter(|entry| {
                            entry
                                .output
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&search)
                        })
                        .cloned()
                        .collect();
                    if entries.is_empty() {
                        self.empty(
                            ui,
                            "No history yet",
                            "Finished queue attempts will appear here.",
                        );
                    }
                    for entry in entries {
                        history_card(ui, &entry);
                    }
                } else {
                    let jobs: Vec<_> = self
                        .jobs
                        .iter()
                        .filter(|job| {
                            self.filter.accepts(job.state())
                                && job
                                    .output()
                                    .to_string_lossy()
                                    .to_lowercase()
                                    .contains(&search)
                        })
                        .cloned()
                        .collect();
                    if jobs.is_empty() {
                        self.empty(
                            ui,
                            "Ready for your next download",
                            "Add a direct file URL and let RustDM take it from here.",
                        );
                    }
                    for job in jobs {
                        self.download_card(ui, &job);
                    }
                }
            });
    }

    fn empty(&mut self, ui: &mut Ui, title: &str, subtitle: &str) {
        ui.add_space(58.0);
        ui.vertical_centered(|ui| {
            download_icon(ui, 68.0);
            ui.add_space(18.0);
            ui.label(RichText::new(title).size(22.0).strong());
            ui.label(RichText::new(subtitle).color(MUTED));
            ui.add_space(18.0);
            if self.filter != Filter::History
                && ui
                    .add(Button::new("+  Add your first download").fill(BLUE))
                    .clicked()
            {
                self.show_add = true;
                self.focus_url = true;
            }
        });
    }

    fn download_card(&mut self, ui: &mut Ui, job: &QueueJob) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let extension = job
                    .output()
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("FILE")
                    .to_uppercase();
                Frame::new()
                    .fill(BLUE.gamma_multiply(0.13))
                    .corner_radius(9)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(extension.chars().take(5).collect::<String>())
                                .small()
                                .strong()
                                .color(BLUE),
                        );
                    });
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(
                            job.output()
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                        )
                        .size(17.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(job.output().display().to_string())
                            .small()
                            .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(job.state().to_string())
                            .small()
                            .color(status_color(job.state())),
                    );
                    if job.state() == JobState::Failed
                        && ui
                            .add_enabled(!self.backend.running(), Button::new("Retry"))
                            .clicked()
                    {
                        self.backend.retry(job.id());
                    }
                });
            });
            ui.add_space(10.0);
            let metrics = self.metrics.get(&job.id());
            let fraction = if job.state() == JobState::Completed {
                1.0
            } else {
                metrics
                    .and_then(|m| {
                        m.progress
                            .total
                            .filter(|n| *n > 0)
                            .map(|total| m.progress.downloaded as f32 / total as f32)
                    })
                    .unwrap_or(0.0)
            };
            ui.add(
                egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                    .desired_height(5.0)
                    .fill(if job.state() == JobState::Completed {
                        GREEN
                    } else {
                        BLUE
                    }),
            );
            ui.horizontal_wrapped(|ui| {
                let message = match (job.state(), metrics) {
                    (JobState::Completed, _) => "Saved successfully".into(),
                    (JobState::Running, Some(m)) => {
                        let total = m
                            .progress
                            .total
                            .map(bytes)
                            .unwrap_or_else(|| "unknown size".into());
                        let speed = m
                            .bytes_per_second
                            .map(|rate| format!("{}/s", bytes(rate as u64)))
                            .unwrap_or_else(|| "Measuring...".into());
                        let eta = m
                            .eta
                            .map(|d| format!("{}s remaining", d.as_secs()))
                            .unwrap_or_else(|| "ETA unknown".into());
                        format!(
                            "{} / {total}    {speed}    {eta}",
                            bytes(m.progress.downloaded)
                        )
                    }
                    (JobState::Running, None) => "Connecting and checking remote file...".into(),
                    (JobState::Paused, _) => "Paused — start the queue to resume".into(),
                    (JobState::Failed, _) => "Download failed — inspect History, then retry".into(),
                    _ => "Waiting in queue".into(),
                };
                ui.label(RichText::new(message).small().color(MUTED));
                if job.sha256().is_some() {
                    ui.label(RichText::new("SHA-256 enabled").small().color(GREEN));
                }
                if let Some(rate) = job.bandwidth_limit() {
                    ui.label(
                        RichText::new(format!("Limit: {}/s", bytes(rate)))
                            .small()
                            .color(MUTED),
                    );
                }
            });
        });
        ui.add_space(4.0);
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_add {
            return;
        }
        let mut open = true;
        egui::Window::new("New download")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(550.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("A URL. A destination. You're all set.").color(MUTED));
                ui.add_space(16.0);
                ui.label("Direct file URL");
                let url_response = ui.add(
                    egui::TextEdit::singleline(&mut self.form.url)
                        .hint_text("https://example.com/file.zip")
                        .desired_width(f32::INFINITY),
                );
                if self.focus_url {
                    url_response.request_focus();
                    self.focus_url = false;
                }
                ui.label("Save as");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form.output)
                            .hint_text("Full path, including the filename")
                            .desired_width(400.0),
                    );
                    if ui
                        .add_enabled(!self.picking, Button::new("Browse..."))
                        .clicked()
                    {
                        let filename = self
                            .form
                            .url
                            .split('?')
                            .next()
                            .unwrap_or("")
                            .rsplit('/')
                            .next()
                            .filter(|s| !s.is_empty())
                            .unwrap_or("download.bin")
                            .to_owned();
                        self.backend.pick_destination(filename);
                        self.picking = true;
                    }
                });
                ui.add_space(8.0);
                egui::CollapsingHeader::new("Transfer options").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Connections");
                        ui.add(egui::Slider::new(&mut self.form.connections, 1..=16));
                    });
                    ui.label("Speed limit · KiB/s (leave blank for unlimited)");
                    ui.text_edit_singleline(&mut self.form.rate);
                    ui.label("Expected SHA-256 (optional)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form.sha256)
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(&mut self.form.resume, "Resume existing partial download");
                });
                if let Some(error) = &self.form.error {
                    ui.colored_label(AMBER, error);
                }
                ui.add_space(14.0);
                ui.add_enabled_ui(!self.adding, |ui| {
                    ui.horizontal(|ui| {
                        let queue = ui.button("Add to queue").clicked();
                        let start = ui.add(Button::new("Download now").fill(BLUE)).clicked();
                        if queue || start {
                            match self.form.options() {
                                Ok(options) => {
                                    self.adding = true;
                                    self.form.error = None;
                                    self.backend.add(options, start);
                                }
                                Err(error) => self.form.error = Some(format!("{error:#}")),
                            }
                        }
                    });
                });
                if self.adding {
                    ui.label(RichText::new("Adding download...").small().color(MUTED));
                }
            });
        if !open && !self.adding {
            self.show_add = false;
        }
    }
}

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::N)) {
            self.show_add = true;
            self.focus_url = true;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.backend.running() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = true;
            self.backend.pause();
        }
        if self.closing && !self.backend.running() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(if self.backend.running() {
            100
        } else {
            1000
        }));
        egui::Panel::left("navigation")
            .default_size(216.0)
            .resizable(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(20))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BACKGROUND).inner_margin(28))
            .show(ui, |ui| self.dashboard(ui));
        self.add_dialog(&ctx);
    }
}

fn card() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(18)
}
fn status_color(state: JobState) -> egui::Color32 {
    match state {
        JobState::Completed => GREEN,
        JobState::Running => BLUE,
        JobState::Failed | JobState::Paused => AMBER,
        _ => MUTED,
    }
}
fn stat(ui: &mut Ui, label: &str, value: &str, color: egui::Color32) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(label).small().color(MUTED));
        ui.add_space(8.0);
        ui.label(RichText::new(value).size(26.0).strong().color(color));
    });
}
fn download_icon(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12, BLUE.gamma_multiply(0.15));
    let c = rect.center();
    let d = size * 0.17;
    let stroke = Stroke::new(2.5, BLUE);
    painter.line_segment([c + egui::vec2(0.0, -d), c + egui::vec2(0.0, d)], stroke);
    painter.line_segment([c + egui::vec2(-d, 0.0), c + egui::vec2(0.0, d)], stroke);
    painter.line_segment([c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d)], stroke);
}
fn history_card(ui: &mut Ui, entry: &HistoryEntry) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(
                entry
                    .output
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            )
            .strong(),
        );
        ui.label(
            RichText::new(format!(
                "Job {} · Attempt {} · {}",
                entry.job_id, entry.attempt, entry.state
            ))
            .small()
            .color(status_color(entry.state)),
        );
        if let Some(error) = &entry.error {
            ui.label(RichText::new(error).small().color(MUTED));
        }
    });
    ui.add_space(4.0);
}
