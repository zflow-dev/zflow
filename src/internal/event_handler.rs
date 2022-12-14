use std::{any::Any, sync::Arc};

use futures::lock::Mutex;

#[derive(Clone)]
pub struct EventActor<T> {
    pub once: bool,
    pub callback: Arc<Mutex<dyn FnMut(&mut T, &dyn Any) -> ()>>
}

//a trait representing an event signal
pub trait EventSignal:Sized {
    /// Send event
    fn emit(&mut self, name:&str, data: &dyn Any);
    /// Attach listener to an event
    fn connect<F>(
        &mut self,
        name: &str,
        rec: F,
        once: bool,
    ) where F: FnMut(&mut Self, &dyn Any) -> () + 'static;
    /// Remove listeners from event
    fn disconnect(&mut self, name: &str);
    /// Check if we have events
    fn has_event(&self, name: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use std::{any::Any, collections::HashMap, boxed::Box, sync::Arc};
    use futures::{lock::Mutex, executor::block_on};
    use crate::internal::event_handler::{EventSignal, EventActor};
    use z_macros::{event_handler_attributes, EventHandler};

    #[event_handler_attributes]
    #[derive(Clone, EventHandler)]
    pub struct MyObject {
        pub name: String,
    }

    impl  MyObject {
        pub fn new(name: &str) -> Self {
            MyObject {
                name:name.to_string(),
                event_counter: 0,
                event_recs: HashMap::new(),
            }
        }
    }

    #[test]
    fn test_event() {
        let x = &0;
        let mut obj = MyObject::new("<My Game Object>");
        let callback = |this: &mut MyObject, data: &dyn Any|  {
            println!(
                "I added this listener {} later...{}",
                this.name,
                data.downcast_ref::<i32>().unwrap()
            );

            // println!("{}", obj.name);
        };

        obj.connect("game", callback, false);

 
        obj.emit("game", &57);
        obj.emit("game", &58);
        // assert_eq!(1, r4.0);
        assert_eq!(obj.has_event("game"), true);
        obj.disconnect("game");
    }
}