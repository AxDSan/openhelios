use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;

use egui::Context;
use ksni::blocking::TrayMethods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Hide,
    Toggle,
    Quit,
}

#[derive(Debug)]
pub enum TrayEvent {
    Command(TrayCommand),
    Ready,
    Unavailable(String),
}

struct OpenHeliosTray {
    events: Sender<TrayEvent>,
    ctx: Context,
}

impl OpenHeliosTray {
    fn send(&self, command: TrayCommand) {
        let _ = self.events.send(TrayEvent::Command(command));
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for OpenHeliosTray {
    fn id(&self) -> String {
        "openhelios".to_string()
    }

    fn title(&self) -> String {
        "OpenHelios".to_string()
    }

    fn icon_name(&self) -> String {
        "input-keyboard".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![tray_icon().clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::Toggle);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Open OpenHelios".to_string(),
                icon_name: "input-keyboard".to_string(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Hide window".to_string(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Hide)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit OpenHelios".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn tray_icon() -> &'static ksni::Icon {
    static ICON: LazyLock<ksni::Icon> = LazyLock::new(|| {
        let image = image::load_from_memory(include_bytes!("../../../assets/logo.png"))
            .expect("embedded OpenHelios icon is valid PNG")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let mut data = image.into_raw();
        // StatusNotifierItem carries ARGB32 in network byte order.
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel.rotate_right(1);
        }
        ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        }
    });
    &ICON
}

/// Register the tray on a background thread.
///
/// `ksni` connects to the session D-Bus and waits for the StatusNotifierWatcher.
/// Doing that here keeps window startup independent of the desktop's tray host.
pub fn spawn(ctx: Context) -> Receiver<TrayEvent> {
    let (events, receiver) = channel();
    let thread_events = events.clone();
    thread::spawn(move || {
        let tray = OpenHeliosTray {
            events: thread_events,
            ctx: ctx.clone(),
        };
        match tray.spawn() {
            Ok(handle) => {
                let _ = events.send(TrayEvent::Ready);
                ctx.request_repaint();
                // Keep the service handle alive; if the desktop's tray service
                // disappears, tell the app so it cannot hide unreachable.
                loop {
                    thread::sleep(Duration::from_secs(2));
                    if handle.is_closed() {
                        let _ =
                            events.send(TrayEvent::Unavailable("tray service stopped".to_string()));
                        ctx.request_repaint();
                        break;
                    }
                }
            }
            Err(error) => {
                let _ = events.send(TrayEvent::Unavailable(error.to_string()));
                ctx.request_repaint();
            }
        }
    });
    receiver
}
