#[cfg(test)]
mod tests {

    use std::collections::HashMap;
    use std::env::current_dir;
    use std::sync::{Arc, Mutex};

    use beady::scenario;
    use poll_promise::Promise;
    use serde_json::Value;
    use zflow_graph::Graph;

    use crate::component::{Component, ComponentOptions, GraphDefinition};
    use crate::loader::{ComponentLoader, ComponentLoaderOptions};
    use crate::process::ProcessResult;
    use crate::registry::{ComponentSource, RuntimeRegistry};

    pub struct TestLoaderRegistry;
    impl RuntimeRegistry for TestLoaderRegistry {
        fn set_source(
            &mut self,
            namespace: &str,
            component_name: &str,
            source: ComponentSource,
        ) -> Result<(), String> {
            todo!()
        }

        fn get_source(&self, component_name: &str) -> Option<ComponentSource> {
            todo!()
        }

        fn get_languages(&mut self) -> Result<Vec<String>, String> {
            todo!()
        }

        fn register(
            &mut self,
            loader: &mut ComponentLoader,
        ) -> Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>> {
            return Promise::spawn_thread("my_registry", || {
                let graph = Graph::new("my_graph", false);
                let test_comp: Box<dyn GraphDefinition> =
                    Box::new(Component::new(ComponentOptions {
                        process: Some(Box::new(|_| Ok(ProcessResult::default()))),
                        ..ComponentOptions::default()
                    }));
                let graph_comp: Box<dyn GraphDefinition> =
                    Box::new(Component::new(ComponentOptions {
                        graph: Some(Box::new(graph)),
                        process: None,
                        ..ComponentOptions::default()
                    }));

                let comp_map = HashMap::from([
                    ("test_component".to_owned(), test_comp),
                    ("Graph".to_owned(), graph_comp),
                ]);
                Ok(comp_map)
            });
        }

        fn dynamic_load(
            &mut self,
            component_name: &str,
            path: &str,
            options: Value,
        ) -> Result<Arc<Mutex<Component>>, String> {
            todo!()
        }
    }

    #[scenario]
    #[test]
    fn test_loader() {
        'given_a_loader: {
            let binding = current_dir().expect("expected current directory");
            let dir = binding.to_str().unwrap();
            let mut l = ComponentLoader::new(
                dir,
                ComponentLoaderOptions::default(),
                Some(Arc::new(Mutex::new(TestLoaderRegistry {}))),
            );
            'then_it_should_initially_know_of_no_components: {
                assert_eq!(l.components.clone().try_lock().unwrap().len(), 0);
            }
            'then_it_should_not_initially_be_processing: {
                assert!(l.processing.is_none());
            }
            'then_it_should_not_be_initially_ready: {
                assert!(!l.ready);
            }

            'then_it_should_be_able_to_read_list_of_components: {
                if let Ok(components) = l.list_components() {
                    assert!(l.processing.is_none(), "should have stopped processing");

                    assert!(
                        !l.components.clone().try_lock().unwrap().is_empty(),
                        "should contain components"
                    );

                    assert!(
                        Arc::ptr_eq(&l.components, &components),
                        "should have returned the full list"
                    );

                    assert!(l.ready, "should have been set ready");
                }
            }
            'then_it_should_have_the_graph_component_registered: {
                l.list_components().unwrap();
                if let Ok(components) = l.components.clone().try_lock() {
                    assert!(components.contains_key("Graph"))
                }
            }
            'then_when_loading_the_graph_component: {
                'and_then_it_should_be_able_to_load_the_component: {
                    let loaded = l.load("Graph", Value::Null);
                    assert!(loaded.is_ok());

                    if let Ok(inst) = loaded.unwrap().clone().try_lock() {
                        assert_eq!(inst.component_name, Some("Graph".to_owned()));
                    }
                }
                'and_then_it_should_contain_input_ports: {
                    let loaded = l.load("Graph", Value::Null);
                    if let Ok(inst) = loaded.unwrap().clone().try_lock() {
                        assert!(!inst.in_ports.ports.is_empty());
                        assert!(inst.in_ports.ports.contains_key("graph"));
                    }
                }
                'and_then_it_should_know_that_graph_is_a_subgraph: {
                    let loaded = l.load("Graph", Value::Null);
                    if let Ok(inst) = loaded.unwrap().clone().try_lock() {
                        assert!(inst.is_subgraph());
                    }
                }
                'and_then_it_should_know_the_description_for_the_graph: {
                    let loaded = l.load("Graph", Value::Null);
                    if let Ok(inst) = loaded.unwrap().clone().try_lock() {
                        assert!(!inst.get_description().is_empty());
                    }
                }
                'and_then_it_should_be_able_to_provide_an_icon_for_the_graph: {
                    let loaded = l.load("Graph", Value::Null);
                    if let Ok(inst) = loaded.unwrap().clone().try_lock() {
                        assert!(!inst.get_icon().is_empty());
                        assert_eq!(inst.get_icon(), "჻".to_owned());
                    }
                }
            }
        }
    }
}
