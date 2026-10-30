use vault_contracts::ports::repository::ChangeAuthorizer;
use vault_contracts::{credentials::*, errors::Result, ports::repository::MutationPolicy};
pub(crate) struct CredentialPolicy<'a> {
    pub authorization: &'a dyn ChangeAuthorizer,
    pub site: super::validation::SiteRule,
}
impl MutationPolicy for CredentialPolicy<'_> {
    fn prepare(&self, value: &mut EntryInput) -> Result<()> {
        super::validation::prepare(value, self.site)
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
        self.authorization.authorize(s, old, new)
    }
}
