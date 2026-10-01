use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct CommandErrorDto {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

impl CommandErrorDto {
    pub(crate) const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}
