use std::{any::Any, sync::Arc};

use futures::lock::Mutex;

pub(crate) type TypeFn<'a, T> = Arc<Mutex<dyn FnMut(&mut T, &dyn Any)->() + 'a>>;


#[derive(Clone)]
pub struct EventActor<'a, T> {
    pub once: bool,
    pub callback: TypeFn<'a, T>
}

pub trait EventManager<'a> {
     /// Send event
     fn emit(&mut self, name:&'a str, data: &dyn Any);
     /// Attach listener to an event
     fn connect(
         &mut self,
         name: &'a str,
         rec: impl FnMut(&mut Self, &dyn Any)->() + 'a,
         once: bool,
     );
     /// Remove listeners from event
     fn disconnect(&mut self, name: &'a str);
     /// Check if we have events
     fn has_event(&self, name: &'a str) -> bool;  
}