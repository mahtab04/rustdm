use anyhow::Result;
use eframe::egui;
use std::sync::mpsc;

#[cfg(target_os = "windows")]
mod windows;

// Other platforms keep normal window-close behavior until they have a native adapter.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Show,
    Start,
    Pause,
    Quit,
}

pub(crate) struct Tray {
    #[cfg(target_os = "windows")]
    native: windows::NativeTray,
    events: mpsc::Receiver<Action>,
}

impl Tray {
    pub(crate) fn new(context: &egui::Context) -> Result<Option<Self>> {
        #[cfg(target_os = "windows")]
        {
            let (sender, events) = mpsc::channel();
            let native = windows::NativeTray::new(context, sender)?;
            Ok(Some(Self { native, events }))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = context;
            Ok(None)
        }
    }

    pub(crate) fn next(&self) -> Option<Action> {
        self.events.try_recv().ok()
    }

    pub(crate) fn update(&mut self, running: bool, pending: bool, closing: bool) {
        #[cfg(target_os = "windows")]
        self.native.update(running, pending, closing);
        #[cfg(not(target_os = "windows"))]
        let _ = (running, pending, closing);
    }
}
