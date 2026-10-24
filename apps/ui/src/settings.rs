use crate::{
    preferences::{Preferences, PreferencesStore},
    theme::AMBER,
};
use anyhow::{Context, Result, ensure};
use eframe::egui;

#[derive(Default)]
pub(crate) struct Settings {
    open: bool,
    draft: Preferences,
    rate: String,
    error: Option<String>,
}

impl Settings {
    pub(crate) fn open(&mut self, preferences: &Preferences) {
        self.draft = preferences.clone();
        self.rate = preferences
            .limit_kib
            .map(|rate| rate.to_string())
            .unwrap_or_default();
        self.error = None;
        self.open = true;
    }

    fn save(&mut self, store: &PreferencesStore) -> Result<()> {
        self.draft.download_directory = self.draft.download_directory.trim().to_owned();
        if !self.draft.download_directory.is_empty() {
            let directory = std::path::Path::new(&self.draft.download_directory);
            ensure!(
                directory.is_absolute() && directory.is_dir(),
                "Enter an existing absolute folder path"
            );
        }
        self.draft.limit_kib = if self.rate.trim().is_empty() {
            None
        } else {
            Some(
                self.rate
                    .trim()
                    .parse()
                    .context("Speed limit must be a positive integer in KiB/s")?,
            )
        };
        store.save(&self.draft)
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        store: &PreferencesStore,
    ) -> Option<Preferences> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut saved = None;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label("Defaults for new downloads. Existing jobs keep their options.");
                ui.add_space(12.0);
                ui.label("Initial file picker folder (optional)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.download_directory)
                        .desired_width(f32::INFINITY)
                        .hint_text("Existing absolute folder path"),
                );
                ui.horizontal(|ui| {
                    ui.label("Connections");
                    ui.add(egui::Slider::new(&mut self.draft.connections, 1..=16));
                });
                ui.label("Speed limit · KiB/s (blank for unlimited)");
                ui.text_edit_singleline(&mut self.rate);
                ui.checkbox(
                    &mut self.draft.resume,
                    "Resume existing partial downloads by default",
                );
                if let Some(error) = &self.error {
                    ui.colored_label(AMBER, error);
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Save settings").clicked() {
                        match self.save(store) {
                            Ok(()) => saved = Some(self.draft.clone()),
                            Err(error) => self.error = Some(format!("{error:#}")),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });
        self.open &= open && saved.is_none();
        saved
    }
}
