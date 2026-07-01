use crate::{config::Binding, input::BindTarget};

#[derive(Debug, Clone)]
pub enum AppEvent {
    BindingCaptured(BindTarget, Binding),
    StartRequested,
    StopRequested,
    ShowWindow,
    QuitRequested,
    Status(String),
    Error(String),
}
