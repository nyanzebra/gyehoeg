use std::{fs::File, io::read_to_string, path::Path};

use crate::{Result, action::Registry, plan::Plan};

pub trait Executor {
    fn spawn<F>(&self, future: F) -> Result<()>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

pub struct Engine(Registry);

impl Engine {
    pub fn new(registry: Registry) -> Self {
        Self(registry)
    }
}

impl Engine {
    pub async fn run(&self, plan: &Path, executor: &impl Executor) -> Result<()> {
        let plan: Plan = serde_yaml::from_str(&read_to_string(File::open(plan)?)?)?;
        let plan = plan.compile(self.0.clone())?;
        plan.exec(executor).await
    }
}
