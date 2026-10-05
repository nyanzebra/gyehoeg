//! Demonstrates running independent jobs in parallel with a Tokio-backed
//! `Executor`. Each job reads two JSON files, merges their contents with a
//! JMESPath expression, selects a few fields out of the merged document, and
//! prints the result.
//!
//! Run with: `cargo run --example pipeline`

use std::future::Future;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;
use tokio::task::JoinHandle;

use gwanri::{Action, Engine, Error, Executor, Registry, Result};

/// Reads a JSON file from disk and parses its contents.
struct ReadJsonFile;

impl Action for ReadJsonFile {
    type Args = String;
    type Output = Value;

    async fn act(&self, path: String) -> Result<Value> {
        let content = tokio::fs::read_to_string(&path).await.map_err(Error::IO)?;
        Ok(serde_json::from_str(&content)?)
    }
}

/// Pretty-prints whatever value it receives, passing it through unchanged.
struct Print;

impl Action for Print {
    type Args = Value;
    type Output = Value;

    async fn act(&self, value: Value) -> Result<Value> {
        println!("{}", serde_json::to_string_pretty(&value)?);
        Ok(value)
    }
}

/// An [`Executor`] that runs jobs as Tokio tasks, keeping their
/// [`JoinHandle`]s so the caller can wait for every job to finish before the
/// process exits.
#[derive(Default)]
struct TokioExecutor {
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl Executor for TokioExecutor {
    fn spawn<F>(&self, future: F) -> Result<()>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = tokio::spawn(async move {
            future.await;
        });
        self.handles.lock().unwrap().push(handle);
        Ok(())
    }
}

impl TokioExecutor {
    /// Waits for every job spawned so far to complete.
    async fn join(&self) {
        let handles = std::mem::take(&mut *self.handles.lock().unwrap());
        for handle in handles {
            if let Err(err) = handle.await {
                eprintln!("job panicked: {err}");
            }
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let registry = Registry::new();
    registry.insert("read_json_file", ReadJsonFile);
    registry.insert("print", Print);

    let engine = Engine::new(registry);
    let executor = TokioExecutor::default();

    engine
        .run(Path::new("examples/data/plan.yaml"), &executor)
        .await?;

    // `Engine::run` only spawns the plan's jobs onto the executor; wait for
    // them to actually finish running before the process exits.
    executor.join().await;

    Ok(())
}
