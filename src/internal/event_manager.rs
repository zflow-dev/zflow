use std::{sync::{Arc, Mutex}};
use std::fmt::Debug;

use serde_json::Value;

pub(crate) type TypeFn<T> = Arc<Mutex<(dyn FnMut(&mut T, Value) -> () + 'static)>>;

#[derive(Clone)]
pub struct EventActor<T> {
    pub once: bool,
    pub callback: TypeFn<T>
}

impl<T> Debug for EventActor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventActor").field("once", &self.once).field("callback", &"[CallbackFn]").finish()
    }
}

pub trait EventManager {
    /// Send event
    fn emit(&mut self, name: &'static str, data: Value);

    /// Remove listeners from event
    fn disconnect(&mut self, name: &'static str);
    /// Check if we have events
    fn has_event(&self, name: &'static str) -> bool;
}

pub trait EventListener {
    /// Attach listener to an event
    fn connect(
        &mut self,
        name: &'static str,
        rec: impl FnMut(&mut Self, Value)->() + 'static,
        once: bool,
    );
}
