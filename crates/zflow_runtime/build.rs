use std::{env, path::PathBuf};

use deno_runtime::ops::bootstrap::SnapshotOptions;

fn main(){
    let o = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let runtime_snapshot_path = o.join("DENO_RUNTIME_SNAPSHOT.bin");
    deno_runtime::snapshot::create_runtime_snapshot(runtime_snapshot_path, SnapshotOptions::default());
}