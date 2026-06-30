//! Shared Rustzen platform conventions.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ServiceLayout {
    product: String,
    install_root: PathBuf,
}

impl ServiceLayout {
    pub fn for_product(product: impl Into<String>) -> Self {
        let product = product.into();
        Self {
            install_root: PathBuf::from(format!("/opt/{product}")),
            product,
        }
    }

    pub fn new(product: impl Into<String>, install_root: impl Into<PathBuf>) -> Self {
        Self {
            product: product.into(),
            install_root: install_root.into(),
        }
    }

    pub fn product(&self) -> &str {
        &self.product
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.install_root.join("bin")
    }

    pub fn bin_path(&self, binary_name: &str) -> PathBuf {
        self.bin_dir().join(binary_name)
    }

    pub fn config_dir(&self) -> PathBuf {
        self.install_root.join("config")
    }

    pub fn env_file(&self) -> PathBuf {
        self.config_dir().join("app.env")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.install_root.join("data")
    }

    pub fn db_dir(&self) -> PathBuf {
        self.data_dir().join("db")
    }

    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir().join("uploads")
    }

    pub fn avatars_dir(&self) -> PathBuf {
        self.data_dir().join("avatars")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.install_root.join("logs")
    }

    pub fn systemd_dir(&self) -> PathBuf {
        self.install_root.join("systemd")
    }

    pub fn service_file_name(&self) -> String {
        format!("{}.service", self.product)
    }

    pub fn service_file_path(&self) -> PathBuf {
        self.systemd_dir().join(self.service_file_name())
    }

    pub fn systemd_unit_path(&self) -> PathBuf {
        PathBuf::from("/etc/systemd/system").join(self.service_file_name())
    }

    pub fn web_dir(&self) -> PathBuf {
        self.install_root.join("web")
    }

    pub fn web_dist_dir(&self) -> PathBuf {
        self.web_dir().join("dist")
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.bin_dir(),
            self.config_dir(),
            self.db_dir(),
            self.uploads_dir(),
            self.avatars_dir(),
            self.logs_dir(),
            self.systemd_dir(),
            self.web_dir(),
        ]
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ResourceLimits {
    pub memory_high: Option<String>,
    pub memory_max: Option<String>,
    pub cpu_quota: Option<String>,
    pub tasks_max: Option<u64>,
    pub limit_nofile: Option<u64>,
}

impl ResourceLimits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn memory_high(mut self, value: impl Into<String>) -> Self {
        self.memory_high = Some(value.into());
        self
    }

    pub fn memory_max(mut self, value: impl Into<String>) -> Self {
        self.memory_max = Some(value.into());
        self
    }

    pub fn cpu_quota(mut self, value: impl Into<String>) -> Self {
        self.cpu_quota = Some(value.into());
        self
    }

    pub fn tasks_max(mut self, value: u64) -> Self {
        self.tasks_max = Some(value);
        self
    }

    pub fn limit_nofile(mut self, value: u64) -> Self {
        self.limit_nofile = Some(value);
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SystemdService {
    pub name: String,
    pub description: String,
    pub exec_start: PathBuf,
    pub working_directory: PathBuf,
    pub environment_file: Option<PathBuf>,
    pub environment: Vec<(String, String)>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub restart: String,
    pub restart_sec: u64,
    pub resource_limits: ResourceLimits,
    pub no_new_privileges: bool,
    pub private_tmp: bool,
}

impl SystemdService {
    pub fn new(name: impl Into<String>, exec_start: impl Into<PathBuf>) -> Self {
        let name = name.into();
        let layout = ServiceLayout::for_product(name.as_str());
        Self::for_layout_with_exec(&layout, exec_start)
    }

    pub fn for_layout(layout: &ServiceLayout, binary_name: &str) -> Self {
        Self::for_layout_with_exec(layout, layout.bin_path(binary_name))
    }

    pub fn for_layout_with_exec(layout: &ServiceLayout, exec_start: impl Into<PathBuf>) -> Self {
        Self {
            name: layout.product().to_string(),
            description: layout.product().to_string(),
            exec_start: exec_start.into(),
            working_directory: layout.install_root().to_path_buf(),
            environment_file: Some(layout.env_file()),
            environment: Vec::new(),
            user: None,
            group: None,
            restart: "always".to_string(),
            restart_sec: 5,
            resource_limits: ResourceLimits::default(),
            no_new_privileges: false,
            private_tmp: false,
        }
    }

    pub fn with_user_group(mut self, user: impl Into<String>, group: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self.group = Some(group.into());
        self
    }

    pub fn with_environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    pub fn with_security(mut self, no_new_privileges: bool, private_tmp: bool) -> Self {
        self.no_new_privileges = no_new_privileges;
        self.private_tmp = private_tmp;
        self
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("[Unit]\n");
        output.push_str(&format!("Description={}\n", self.description));
        output.push_str("After=network.target\n\n");
        output.push_str("[Service]\n");
        output.push_str("Type=simple\n");
        if let Some(user) = &self.user {
            output.push_str(&format!("User={user}\n"));
        }
        if let Some(group) = &self.group {
            output.push_str(&format!("Group={group}\n"));
        }
        if let Some(environment_file) = &self.environment_file {
            output.push_str(&format!("EnvironmentFile={}\n", environment_file.display()));
        }
        for (key, value) in &self.environment {
            output.push_str(&format!("Environment={key}={value}\n"));
        }
        output.push_str(&format!("ExecStart={}\n", self.exec_start.display()));
        output.push_str(&format!(
            "WorkingDirectory={}\n",
            self.working_directory.display()
        ));
        output.push_str(&format!("Restart={}\n", self.restart));
        output.push_str(&format!("RestartSec={}\n", self.restart_sec));
        if let Some(memory_high) = &self.resource_limits.memory_high {
            output.push_str(&format!("MemoryHigh={memory_high}\n"));
        }
        if let Some(memory_max) = &self.resource_limits.memory_max {
            output.push_str(&format!("MemoryMax={memory_max}\n"));
        }
        if let Some(cpu_quota) = &self.resource_limits.cpu_quota {
            output.push_str(&format!("CPUQuota={cpu_quota}\n"));
        }
        if let Some(tasks_max) = self.resource_limits.tasks_max {
            output.push_str(&format!("TasksMax={tasks_max}\n"));
        }
        if let Some(limit_nofile) = self.resource_limits.limit_nofile {
            output.push_str(&format!("LimitNOFILE={limit_nofile}\n"));
        }
        if self.no_new_privileges {
            output.push_str("NoNewPrivileges=true\n");
        }
        if self.private_tmp {
            output.push_str("PrivateTmp=true\n");
        }
        output.push_str("\n[Install]\n");
        output.push_str("WantedBy=multi-user.target\n");
        output
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeploymentPlan {
    pub layout: ServiceLayout,
    pub service: SystemdService,
    pub env_entries: Vec<(String, String)>,
}

impl DeploymentPlan {
    pub fn new(layout: ServiceLayout, binary_name: &str) -> Self {
        let service = SystemdService::for_layout(&layout, binary_name);
        Self {
            layout,
            service,
            env_entries: Vec::new(),
        }
    }

    pub fn with_service(mut self, service: SystemdService) -> Self {
        self.service = service;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_entries.push((key.into(), value.into()));
        self
    }

    pub fn render_env_file(&self) -> String {
        render_env_file(
            self.env_entries
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
    }

    pub fn render_service_file(&self) -> String {
        self.service.render()
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        self.layout.required_dirs()
    }
}

pub fn render_env_file(
    entries: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
) -> String {
    let mut output = String::new();
    for (key, value) in entries {
        output.push_str(key.as_ref());
        output.push('=');
        output.push_str(value.as_ref());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{DeploymentPlan, ResourceLimits, ServiceLayout, SystemdService, render_env_file};
    use std::path::PathBuf;

    #[test]
    fn layout_uses_opt_product_root() {
        let layout = ServiceLayout::for_product("rustzen-admin");
        assert_eq!(
            layout.install_root().to_path_buf(),
            PathBuf::from("/opt/rustzen-admin")
        );
        assert_eq!(
            layout.env_file(),
            PathBuf::from("/opt/rustzen-admin/config/app.env")
        );
        assert_eq!(layout.service_file_name(), "rustzen-admin.service");
        assert_eq!(
            layout.systemd_unit_path(),
            PathBuf::from("/etc/systemd/system/rustzen-admin.service")
        );
    }

    #[test]
    fn custom_layout_is_preserved_in_systemd_render() {
        let layout = ServiceLayout::new("rustzen-admin", "/srv/rustzen-admin");
        let service = SystemdService::for_layout(&layout, "rustzen-admin").render();
        assert!(service.contains("EnvironmentFile=/srv/rustzen-admin/config/app.env"));
        assert!(service.contains("ExecStart=/srv/rustzen-admin/bin/rustzen-admin"));
        assert!(service.contains("WorkingDirectory=/srv/rustzen-admin"));
    }

    #[test]
    fn systemd_render_has_required_fields() {
        let layout = ServiceLayout::for_product("rustzen-admin");
        let limits = ResourceLimits::new()
            .memory_high("4G")
            .memory_max("6G")
            .cpu_quota("300%");
        let service = SystemdService::for_layout(&layout, "rustzen-admin")
            .with_security(true, true)
            .with_resource_limits(limits)
            .render();
        assert!(service.contains("EnvironmentFile=/opt/rustzen-admin/config/app.env"));
        assert!(service.contains("ExecStart=/opt/rustzen-admin/bin/rustzen-admin"));
        assert!(service.contains("Restart=always"));
        assert!(service.contains("MemoryHigh=4G"));
        assert!(service.contains("CPUQuota=300%"));
        assert!(service.contains("NoNewPrivileges=true"));
        assert!(service.contains("PrivateTmp=true"));
    }

    #[test]
    fn env_file_renderer_is_stable() {
        let env = render_env_file([("RUSTZEN_APP_PORT", "9880"), ("RUSTZEN_RUNTIME_ROOT", ".")]);
        assert_eq!(env, "RUSTZEN_APP_PORT=9880\nRUSTZEN_RUNTIME_ROOT=.\n");
    }

    #[test]
    fn deployment_plan_renders_env_and_service() {
        let plan = DeploymentPlan::new(ServiceLayout::for_product("rustzen-admin"), "rustzen-admin")
            .with_env("RUSTZEN_RUNTIME_ROOT", ".")
            .with_env("RUSTZEN_APP_PORT", "9880");
        assert!(
            plan.render_service_file()
                .contains("ExecStart=/opt/rustzen-admin/bin/rustzen-admin")
        );
        assert_eq!(
            plan.render_env_file(),
            "RUSTZEN_RUNTIME_ROOT=.\nRUSTZEN_APP_PORT=9880\n"
        );
        assert!(plan.required_dirs().contains(&PathBuf::from("/opt/rustzen-admin/config")));
    }
}
