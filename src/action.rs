use std::{future::Future, sync::Arc};

use dashmap::DashMap;
use serde::{Serialize, de::DeserializeOwned};

use super::Result;

#[derive(Clone)]
pub struct Registry(Arc<DashMap<String, Arc<dyn sealed::TypeErasedAction + Send + Sync>>>);

impl Registry {
    pub fn new() -> Self {
        Self(Arc::new(DashMap::new()))
    }

    pub fn insert(&self, key: impl ToString, value: impl Action + Send + Sync + 'static) -> bool {
        let key = key.to_string();
        let value = Arc::new(value);
        self.0.insert(key, value as _).is_some()
    }

    pub(crate) fn get(
        &self,
        key: impl ToString,
    ) -> Option<Arc<dyn sealed::TypeErasedAction + Send + Sync>> {
        self.0.get(&key.to_string()).map(|x| x.clone())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

pub trait Action {
    type Args: DeserializeOwned;
    type Output: Serialize;

    fn act(&self, args: Self::Args) -> impl Future<Output = Result<Self::Output>> + Send;
}

pub(crate) mod sealed {
    use std::pin::Pin;

    use super::Action;
    use crate::Result;

    pub(crate) trait TypeErasedAction {
        fn act_erased<'a>(
            &'a self,
            args: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;
    }

    impl<A> TypeErasedAction for A
    where
        A: Action + Send + Sync,
    {
        fn act_erased(
            &self,
            args: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async move {
                let args: A::Args = serde_json::from_value(args)?;
                let out = self.act(args).await?;
                let value = serde_json::to_value(out)?;
                Ok(value)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    type Result = std::result::Result<String, String>;

    #[derive(Deserialize, Serialize)]
    struct MyArgs(Result);

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    struct MyOutput(String);

    struct MyAction;

    impl Action for MyAction {
        type Args = MyArgs;
        type Output = MyOutput;

        async fn act(&self, args: Self::Args) -> std::result::Result<Self::Output, crate::Error> {
            args.0
                .map(MyOutput)
                .map_err(|err| crate::Error::Custom(err.into()))
        }
    }

    #[test_case::test_case(MyArgs(Err("Bad".into())), Err("Custom Bad"))]
    #[test_case::test_case(MyArgs(Ok("Cats".into())), Ok(MyOutput("Cats".into())))]
    #[tokio::test]
    async fn action(args: MyArgs, expect: std::result::Result<MyOutput, &str>) {
        let action = MyAction;
        assert_eq!(
            action.act(args).await.map_err(|err| err.to_string()),
            expect.map_err(|err| err.to_string())
        );
    }
}
