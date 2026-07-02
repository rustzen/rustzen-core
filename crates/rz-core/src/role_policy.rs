//! Product-neutral Rustzen built-in role policy helpers.

pub const SYSTEM_WILDCARD: &str = "*";
pub const OWNER_ROLE_CODE: &str = "owner";
pub const ADMIN_ROLE_CODE: &str = "admin";
pub const VIEWER_ROLE_CODE: &str = "viewer";
pub const DEFAULT_DEPLOY_CAPABILITY_PREFIX: &str = "manage:deploy:";
pub const DEFAULT_DEPLOY_VIEW_CAPABILITY: &str = "manage:deploy:list";
pub const VIEW_ACTIONS: &[&str] = &["list", "view", "options"];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RolePolicy {
    deploy_capability_prefix: String,
    deploy_view_capability: String,
    view_actions: Vec<String>,
}

impl RolePolicy {
    pub fn new(
        deploy_capability_prefix: impl Into<String>,
        deploy_view_capability: impl Into<String>,
        view_actions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            deploy_capability_prefix: deploy_capability_prefix.into(),
            deploy_view_capability: deploy_view_capability.into(),
            view_actions: view_actions.into_iter().map(Into::into).collect(),
        }
    }

    pub fn rustzen_default() -> Self {
        Self::new(
            DEFAULT_DEPLOY_CAPABILITY_PREFIX,
            DEFAULT_DEPLOY_VIEW_CAPABILITY,
            VIEW_ACTIONS.iter().copied(),
        )
    }

    pub fn deploy_capability_prefix(&self) -> &str {
        &self.deploy_capability_prefix
    }

    pub fn deploy_view_capability(&self) -> &str {
        &self.deploy_view_capability
    }

    pub fn role_allows_capability(&self, role_code: &str, capability_code: &str) -> bool {
        match role_code {
            OWNER_ROLE_CODE => capability_code == SYSTEM_WILDCARD,
            ADMIN_ROLE_CODE => self.is_assignable_leaf_capability(capability_code),
            VIEWER_ROLE_CODE => {
                self.is_assignable_leaf_capability(capability_code)
                    && self.is_view_capability(capability_code)
            }
            _ => false,
        }
    }

    pub fn is_assignable_leaf_capability(&self, capability_code: &str) -> bool {
        capability_code != SYSTEM_WILDCARD
            && !capability_code.ends_with(":*")
            && !self.is_deploy_operation_capability(capability_code)
    }

    pub fn is_view_capability(&self, capability_code: &str) -> bool {
        capability_code.rsplit(':').next().is_some_and(|action| {
            self.view_actions
                .iter()
                .any(|view_action| view_action == action)
        })
    }

    pub fn is_deploy_capability(&self, capability_code: &str) -> bool {
        capability_code == format!("{}*", self.deploy_capability_prefix)
            || capability_code.starts_with(&self.deploy_capability_prefix)
    }

    pub fn is_deploy_operation_capability(&self, capability_code: &str) -> bool {
        self.is_deploy_capability(capability_code) && capability_code != self.deploy_view_capability
    }
}

impl Default for RolePolicy {
    fn default() -> Self {
        Self::rustzen_default()
    }
}

pub fn default_role_allows_capability(role_code: &str, capability_code: &str) -> bool {
    RolePolicy::default().role_allows_capability(role_code, capability_code)
}

pub fn default_role_capability_codes<'a>(
    role_code: &str,
    capability_codes: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let policy = RolePolicy::default();
    capability_codes
        .into_iter()
        .filter(|capability_code| policy.role_allows_capability(role_code, capability_code))
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ADMIN_ROLE_CODE, OWNER_ROLE_CODE, RolePolicy, SYSTEM_WILDCARD, VIEWER_ROLE_CODE,
        default_role_capability_codes,
    };

    #[test]
    fn owner_only_receives_wildcard() {
        let policy = RolePolicy::default();

        assert!(policy.role_allows_capability(OWNER_ROLE_CODE, SYSTEM_WILDCARD));
        assert!(!policy.role_allows_capability(OWNER_ROLE_CODE, "system:user:list"));
    }

    #[test]
    fn admin_receives_leaf_capabilities_and_deploy_view_only() {
        let policy = RolePolicy::default();

        assert!(policy.role_allows_capability(ADMIN_ROLE_CODE, "system:user:create"));
        assert!(policy.role_allows_capability(ADMIN_ROLE_CODE, "manage:deploy:list"));
        assert!(!policy.role_allows_capability(ADMIN_ROLE_CODE, "manage:deploy:run"));
        assert!(!policy.role_allows_capability(ADMIN_ROLE_CODE, "manage:deploy:*"));
        assert!(!policy.role_allows_capability(ADMIN_ROLE_CODE, SYSTEM_WILDCARD));
    }

    #[test]
    fn viewer_receives_view_class_capabilities_only() {
        let policy = RolePolicy::default();

        assert!(policy.role_allows_capability(VIEWER_ROLE_CODE, "dashboard:view"));
        assert!(policy.role_allows_capability(VIEWER_ROLE_CODE, "system:user:list"));
        assert!(policy.role_allows_capability(VIEWER_ROLE_CODE, "manage:dict:options"));
        assert!(policy.role_allows_capability(VIEWER_ROLE_CODE, "manage:deploy:list"));
        assert!(!policy.role_allows_capability(VIEWER_ROLE_CODE, "system:user:create"));
        assert!(!policy.role_allows_capability(VIEWER_ROLE_CODE, "manage:deploy:run"));
    }

    #[test]
    fn default_role_capability_codes_filters_catalog() {
        let codes = [
            SYSTEM_WILDCARD,
            "system:user:list",
            "system:user:create",
            "manage:deploy:list",
            "manage:deploy:run",
        ];

        assert_eq!(
            default_role_capability_codes(VIEWER_ROLE_CODE, codes),
            vec![
                "system:user:list".to_string(),
                "manage:deploy:list".to_string()
            ]
        );
    }
}
