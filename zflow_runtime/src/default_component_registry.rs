use std::collections::HashMap;

use poll_promise::Promise;

use crate::{loader::{RuntimeRegistry, ComponentLoader, ComponentDefinition, ComponentSource},graph_component::GraphComponentTrait, component::ComponentTrait};

#[derive(Default, Clone)]
pub struct DefaultRegistry<T:ComponentTrait + ?GraphComponentTrait> {
    _data:T
}

unsafe impl<T: ComponentTrait + ?GraphComponentTrait> Send for DefaultRegistry<T>{}

impl<T:ComponentTrait + ?GraphComponentTrait> RuntimeRegistry<T> for DefaultRegistry<T>{
    fn set_source(&mut self,namespace: &str,component_name: &str,source: ComponentSource,) -> Result<(),String>  {
        todo!()
    }

    fn get_source(&self,component_name: &str) -> Option<ComponentSource>  {
        todo!()
    }

    fn get_languages(&mut self) -> Result<Vec<String> ,String>  {
        todo!()
    }

    fn register(&mut self, loader:&mut ComponentLoader<T>) -> Promise<Result<HashMap<String, &'static (dyn ComponentDefinition<T> + Send + Sync)>, String>>  {
        todo!()
    }

    fn dynamic_load(&mut self, component_name: &str, path: &str) -> Result<std::sync::Arc<std::sync::Mutex<T>>, String> {
        todo!()
    }
}
