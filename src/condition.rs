use serde::{Deserialize, Serialize};

use super::{Error, Result};

#[derive(Deserialize, Serialize)]
pub(crate) struct Condition(String);

impl Condition {
    pub(crate) fn compile(self) -> Result<jmespatch::Expression<'static>> {
        jmespatch::compile(&self.0).map_err(Error::JMESPath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case::test_case("user.name == 'bob'")]
    fn compile(value: &str) -> Result<()> {
        Condition(value.into()).compile().map(|_| ())
    }
}
