use crate::{config::Binding, input::BindTarget};

#[derive(Debug, Clone)]
pub enum AppEvent {
    BindingCaptured(BindTarget, Binding),
    #[cfg(not(target_os = "linux"))]
    StartRequested,
    #[cfg(not(target_os = "linux"))]
    StopRequested,
    ShowWindow,
    #[cfg(not(target_os = "linux"))]
    QuitRequested,
    Status(String),
    Error(String),
}
