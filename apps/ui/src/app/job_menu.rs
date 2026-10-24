use super::*;

impl DesktopApp {
    pub(super) fn job_menu(&mut self, ui: &mut Ui, job: &QueueJob) {
        ui.set_min_width(210.0);
        let available = !self.closing && !self.mutating.contains(&job.id());
        let can_start = available && !self.backend.running() && job.state() != JobState::Completed;
        let label = match job.state() {
            JobState::Paused => "Resume download",
            JobState::Failed => "Retry download",
            _ => "Start download",
        };
        if ui
            .add_enabled(can_start, Button::new(label))
            .on_disabled_hover_text("Pause the running queue before starting a selected file.")
            .clicked()
        {
            self.backend.start_job(job.id());
            ui.close();
        }
        let can_pause = available
            && !job.is_held()
            && match job.state() {
                JobState::Running => self.backend.running(),
                JobState::Pending | JobState::Paused => true,
                _ => false,
            };
        if ui
            .add_enabled(can_pause, Button::new("Pause download"))
            .clicked()
        {
            self.mutate_job(job.id(), JobAction::Pause);
            ui.close();
        }
        ui.separator();
        if ui.button("Copy download URL").clicked() {
            ui.ctx().copy_text(job.url().to_owned());
            ui.close();
        }
        if ui.button("Copy file path").clicked() {
            ui.ctx().copy_text(job.output().display().to_string());
            ui.close();
        }
        ui.separator();
        let stopped = available && job.state() != JobState::Running;
        if ui
            .add_enabled(stopped, Button::new("Remove from list"))
            .on_hover_text("Keeps downloaded files, partial data, and attempt history.")
            .clicked()
        {
            self.mutate_job(job.id(), JobAction::Remove);
            ui.close();
        }
        if ui
            .add_enabled(
                stopped,
                Button::new(RichText::new("Delete files...").color(AMBER)),
            )
            .on_hover_text(
                "Delete the downloaded file or verified partial data, then remove this row.",
            )
            .clicked()
        {
            self.pending_delete = Some(job.clone());
            self.delete_ready = false;
            ui.ctx().request_repaint();
            ui.close();
        }
    }

    fn mutate_job(&mut self, id: u64, action: JobAction) {
        if !self.closing && self.mutating.insert(id) {
            self.backend.job_action(id, action);
        }
    }

    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.delete_ready {
            return;
        }
        let Some(job) = self.pending_delete.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Delete download files?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("This permanently deletes this download's saved file or partial data.");
                ui.add_space(10.0);
                ui.label(RichText::new(job.output().display().to_string()).strong());
                ui.add_space(10.0);
                ui.label("The download leaves the list. Attempt history is retained.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_delete = None;
                    }
                    if ui
                        .add(Button::new("Delete files").fill(AMBER.gamma_multiply(0.25)))
                        .clicked()
                    {
                        self.pending_delete = None;
                        self.mutate_job(job.id(), JobAction::Delete);
                    }
                });
            });
        if !open {
            self.pending_delete = None;
        }
    }
}
