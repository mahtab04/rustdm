use crate::{
    model::bytes,
    theme::{AMBER, BLUE, GREEN, MUTED},
};
use eframe::egui::{self, RichText, Ui};
use rustdm_core::{ConnectionProgress, ConnectionStatus};

pub(crate) fn show(ui: &mut Ui, job_id: u64, workers: Option<&[ConnectionProgress]>) {
    ui.add_space(8.0);
    egui::CollapsingHeader::new("Connection details")
        .id_salt(("connections", job_id))
        .default_open(true)
        .show(ui, |ui| {
            let Some(workers) = workers.filter(|workers| !workers.is_empty()) else {
                ui.label(
                    RichText::new("Waiting for live connection updates...")
                        .small()
                        .color(MUTED),
                );
                return;
            };
            egui::ScrollArea::vertical()
                .id_salt(("connection_scroll", job_id))
                .max_height(160.0)
                .show(ui, |ui| {
                    egui::Grid::new(("connection_table", job_id))
                        .num_columns(3)
                        .striped(true)
                        .spacing([36.0, 4.0])
                        .min_row_height(16.0)
                        .min_col_width(80.0)
                        .show(ui, |ui| {
                            for label in ["Connection", "Downloaded", "Status"] {
                                ui.label(RichText::new(label).small().strong().color(MUTED));
                            }
                            ui.end_row();
                            for worker in workers {
                                ui.label(
                                    RichText::new(format!("{:02}", worker.id))
                                        .monospace()
                                        .small()
                                        .color(BLUE),
                                );
                                ui.label(
                                    RichText::new(bytes(worker.downloaded)).monospace().small(),
                                );
                                let (label, color) = match worker.status {
                                    ConnectionStatus::Waiting => ("Waiting for work", MUTED),
                                    ConnectionStatus::Sending => ("Sending request...", BLUE),
                                    ConnectionStatus::Receiving => ("Receiving data...", BLUE),
                                    ConnectionStatus::Retrying => ("Retrying connection...", AMBER),
                                    ConnectionStatus::Completed => ("Completed", GREEN),
                                    ConnectionStatus::Paused => ("Paused", AMBER),
                                    ConnectionStatus::Failed => ("Failed", AMBER),
                                };
                                ui.label(RichText::new(label).small().color(color));
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(6.0);
            ui.label(
                RichText::new("New bytes this attempt. Connections can process multiple ranges.")
                    .small()
                    .color(MUTED),
            );
        });
}
