//! Least-privilege side effects.
//!
//! Authorization reads the policy allow-list and the requested class.
//! It has no budget parameter. A third-down pass cannot spend its way past it.

#![forbid(unsafe_code)]

use playbook_core::{PlaybookError, SideEffectClass};

pub fn authorize(class: SideEffectClass, allowed: &[SideEffectClass]) -> Result<(), PlaybookError> {
    if allowed.contains(&class) {
        Ok(())
    } else {
        Err(PlaybookError::SideEffect(format!("{class:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_budget_does_not_create_a_deployment_permission() {
        let allowed = [SideEffectClass::ReadOnly, SideEffectClass::LocalMutation];
        let low_budget_ppm = 1_u32;
        let first = authorize(SideEffectClass::Deployment, &allowed);
        let second = authorize(SideEffectClass::Deployment, &allowed);
        assert_eq!(first, second);
        assert!(first.is_err());
        assert!(low_budget_ppm < 500_000);
    }
}
