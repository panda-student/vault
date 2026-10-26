use crate::definitions::{
    credentials::*,
    errors::{AppError, Result},
    ports::repository::MutationPolicy,
};
use crate::services::permissions::{self, Access, Caller};
pub(crate) struct CredentialPolicy {
    pub caller: Caller,
    pub access: Access,
}
impl MutationPolicy for CredentialPolicy {
    fn prepare(&self, value: &mut EntryInput) -> Result<()> {
        value.title = value.title.trim().into();
        if value.kind == Kind::Api
            && !value.url.is_empty()
            && value
                .services
                .first()
                .is_none_or(|first| first.url.trim() != value.url.trim())
        {
            return Err(
                AppError::invalid().with("url", "API 凭据的 url 必须与第一个服务地址一致。")
            );
        }
        super::validation::normalize_services(value)?;
        super::validation::validate(value)
    }
    fn patch(&self, current: EntryInput, patch: EntryPatch) -> Result<EntryInput> {
        super::validation::apply_patch(current, patch)
    }
    fn authorize(
        &self,
        s: &Settings,
        old: Option<&EntrySummary>,
        new: &EntrySummary,
    ) -> Result<()> {
        permissions::check_change(self.caller, s, old, new, self.access)
    }
}
