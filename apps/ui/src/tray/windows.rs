use super::Action;
use anyhow::Result;
use eframe::egui;
use std::sync::mpsc;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

/// Native objects live on the GUI thread, whose event loop dispatches their callbacks.
pub(super) struct NativeTray {
    icon: TrayIcon,
    start: MenuItem,
    pause: MenuItem,
    quit: MenuItem,
    state: Option<(bool, bool, bool)>,
}

impl NativeTray {
    pub(super) fn new(context: &egui::Context, sender: mpsc::Sender<Action>) -> Result<Self> {
        let menu = Menu::new();
        let show = MenuItem::new("Show RustDM", true, None);
        let start = MenuItem::new("Start queue", false, None);
        let pause = MenuItem::new("Pause queue", false, None);
        let quit = MenuItem::new("Quit RustDM", true, None);
        menu.append_items(&[
            &show,
            &start,
            &pause,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("RustDM — ready")
            .with_icon(Icon::from_rgba(icon_rgba(), 32, 32)?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        let id = icon.id().clone();
        let click_sender = sender.clone();
        let click_context = context.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if event.id() == &id
                && matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                )
            {
                let _ = click_sender.send(Action::Show);
                click_context.request_repaint();
            }
        }));
        let items = [
            (show.id().clone(), Action::Show),
            (start.id().clone(), Action::Start),
            (pause.id().clone(), Action::Pause),
            (quit.id().clone(), Action::Quit),
        ];
        let context = context.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, action)) = items.iter().find(|(id, _)| id == &event.id) {
                let _ = sender.send(*action);
                context.request_repaint();
            }
        }));
        Ok(Self {
            icon,
            start,
            pause,
            quit,
            state: None,
        })
    }

    pub(super) fn update(&mut self, running: bool, pending: bool, closing: bool) {
        let state = (running, pending, closing);
        if self.state == Some(state) {
            return;
        }
        self.start.set_enabled(!running && pending && !closing);
        self.pause.set_enabled(running && !closing);
        self.quit.set_enabled(!closing);
        let label = if closing {
            "saving progress"
        } else if running {
            "downloading"
        } else {
            "ready"
        };
        let _ = self.icon.set_tooltip(Some(format!("RustDM — {label}")));
        self.state = Some(state);
    }
}

fn icon_rgba() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0_i32..32 {
        for x in 0_i32..32 {
            let shaft = (14..=17).contains(&x) && (6..=20).contains(&y);
            let head = (15..=23).contains(&y) && ((x - 15).abs() - (23 - y)).abs() <= 2;
            let base = (8..=24).contains(&x) && (26..=28).contains(&y);
            pixels.extend_from_slice(if shaft || head || base {
                &[103, 157, 255, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    pixels
}
