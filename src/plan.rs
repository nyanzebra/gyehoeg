use serde::{Deserialize, Serialize};

use crate::{Error, Result, action::Registry, condition::Condition, expression::Expression};

#[derive(Deserialize, Serialize)]
pub(crate) struct Plan {
    jobs: Vec<Job>,
}

impl Plan {
    pub(crate) fn compile(self, symbols: Registry) -> Result<compiled::Plan> {
        let jobs = self
            .jobs
            .into_iter()
            .map(|job| job.compile(symbols.clone()))
            .collect::<Result<Vec<_>>>()?;
        Ok(compiled::Plan(jobs))
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Job {
    steps: Vec<Step>,
}

impl Job {
    fn compile(self, symbols: Registry) -> Result<compiled::Job> {
        let steps = self
            .steps
            .into_iter()
            .map(|step| step.compile(symbols.clone()))
            .collect::<Result<Vec<_>>>()?;
        Ok(compiled::Job(steps))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Step {
    Action {
        id: String,
        action: String,
        /// A JMESPath expression evaluated against the job's accumulated
        /// context (every prior step's output, keyed by alias) to build this
        /// action's arguments. Defaults to the whole context when omitted.
        args: Option<Expression>,
        when: Option<Condition>,
        #[serde(alias = "as")]
        alias: Option<String>,
    },
    Expression {
        id: String,
        expression: Expression,
        when: Option<Condition>,
        #[serde(alias = "as")]
        alias: Option<String>,
    },
}

impl Step {
    fn compile(self, symbols: Registry) -> Result<compiled::Step> {
        match self {
            Step::Action {
                id,
                action,
                args,
                when,
                alias,
            } => {
                let action = symbols
                    .get(&action)
                    .ok_or(Error::Compile(format!("symbol {action} not found")))?;
                let args = args
                    .map(Expression::compile)
                    .transpose()?
                    .map(compiled::SendExpression::from);
                let when = when
                    .map(Condition::compile)
                    .transpose()?
                    .map(compiled::SendExpression::from);

                Ok(compiled::Step::Action {
                    id: id.clone(),
                    alias: alias.unwrap_or(id),
                    action,
                    args,
                    when,
                })
            }
            Step::Expression {
                id,
                expression,
                when,
                alias,
            } => {
                let when = when
                    .map(Condition::compile)
                    .transpose()?
                    .map(compiled::SendExpression::from);
                Ok(compiled::Step::Expression {
                    id: id.clone(),
                    alias: alias.unwrap_or(id),
                    expression: expression.compile()?.into(),
                    when,
                })
            }
        }
    }
}

mod compiled {

    use std::sync::Arc;

    use serde_json::{Map, Value};

    use crate::{Executor, Result, action::sealed::TypeErasedAction};

    pub(crate) struct Plan(pub(super) Vec<Job>);

    impl Plan {
        pub(crate) async fn exec(self, executor: &impl Executor) -> Result<()> {
            for job in self.0.into_iter() {
                executor.spawn(async move {
                    if let Err(err) = job.exec().await {
                        log::error!("Job failed due to {err}");
                    }
                })?;
            }
            Ok(())
        }
    }

    pub(super) struct Job(pub(super) Vec<Step>);

    impl Job {
        async fn exec(self) -> Result<()> {
            let mut context = Map::new();
            for step in self.0 {
                match step.exec(&context).await? {
                    Progress::Value { alias, value } => {
                        log::debug!("Saving {alias} with content {value}");
                        context.insert(alias, value);
                    }
                    Progress::Halt { id } => {
                        log::warn!("Job halted at {id}");
                        break;
                    }
                }
            }
            Ok(())
        }
    }

    pub(super) enum Step {
        Action {
            id: String,
            alias: String,
            action: Arc<dyn TypeErasedAction + Send + Sync>,
            args: Option<SendExpression>,
            when: Option<SendExpression>,
        },
        Expression {
            id: String,
            alias: String,
            expression: SendExpression,
            when: Option<SendExpression>,
        },
    }

    enum Progress {
        Value { alias: String, value: Value },
        Halt { id: String },
    }

    pub(super) struct SendExpression(jmespatch::Expression<'static>);

    unsafe impl Send for SendExpression {}

    impl SendExpression {
        fn is_truthy(&self, context: &Value) -> Result<bool> {
            Ok(self.0.search(context)?.is_truthy())
        }

        fn search(&self, context: &Value) -> Result<Value> {
            Ok(serde_json::to_value(self.0.search(context)?)?)
        }
    }

    impl From<jmespatch::Expression<'static>> for SendExpression {
        fn from(expression: jmespatch::Expression<'static>) -> Self {
            Self(expression)
        }
    }

    impl Step {
        async fn exec(self, context: &Map<String, Value>) -> Result<Progress> {
            let context = Value::Object(context.clone());
            match self {
                Self::Action {
                    id,
                    alias,
                    action,
                    args,
                    when,
                } => {
                    if let Some(when) = when.as_ref()
                        && !when.is_truthy(&context)?
                    {
                        return Ok(Progress::Halt { id });
                    }

                    let args = match args.as_ref() {
                        Some(args) => args.search(&context)?,
                        None => context.clone(),
                    };

                    let value = action.act_erased(args).await?;
                    Ok(Progress::Value { alias, value })
                }
                Self::Expression {
                    id,
                    alias,
                    expression,
                    when,
                } => {
                    if let Some(when) = when.as_ref()
                        && !when.is_truthy(&context)?
                    {
                        return Ok(Progress::Halt { id });
                    }

                    let value = expression.search(&context)?;
                    Ok(Progress::Value { alias, value })
                }
            }
        }
    }
}
