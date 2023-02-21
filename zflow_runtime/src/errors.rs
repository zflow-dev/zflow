use std::pin::Pin;

use futures::{future::join_all, Future};

pub async fn async_transform(futures:Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>>) -> Result<(), String> {
    let mut errors = String::new();
    for v in join_all(futures).await.iter() {
        match v {
             Err(msg) => {
                errors.push_str(msg);
                errors.push('\n');
             },
             _ =>{}
        }
    }
    if !errors.is_empty(){
        return Err(errors);
    }
    Ok(())
}