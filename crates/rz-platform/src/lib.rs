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
        Self { install_root: PathBuf::from(format!("/opt/{product}")), product }
    }

    pub fn new(product: impl Into<String>, install_root: impl Into<PathBuf>) -> Self {
        Self { product: product.into(), install_root: install_root.into() }
    }

    pub fn product(&self) -> &str { &self.product }
    pub fn install_root(&self) -> &Path { &self.install_root }
    pub fn bin_dir(&self) -> PathBuf { self.install_root.join("bin") }
    pub fn config_dir(&self) -> PathBuf { self.install_root.join("config") }
    pub fn env_file(&self) -> PathBuf { self.config_dir().join("app.env") }
    pub fn data_dir(&self) -> PathBuf { self.install_root.join("data") }
    pub fn db_dir(&self) -> PathBuf { self.data_dir().join("db") }
    pub fn logs_dir(&self) -> PathBuf { self.install_root.join("logs") }
    pub fn systemd_dir(&self) -> PathBuf { self.install_root.join("systemd") }
    pub fn web_dist_dir(&self) -> PathBuf { self.install_root.join("web").join("dist") }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.bin_dir(),
            self.config_dir(),
            self.db_dir(),
            self.data_dir().join("uploads"),
            self.data_dir().join("avatars"),
            self.logs_dir(),
            self.systemd_dir(),
            self.install_root.join("web"),
        ]
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
    pub memory_high: Option<String>,
    pub memory_max: Option<String>,
    pub cpu_quota: Option<String>,
    pub tasks_max: Option<u64>,
    pub limit_nofile: Option<u64>,
}

impl SystemdService {
    pub fn new(name: impl Into<String>, exec_start: impl Into<PathBuf>) -> Self {
        let name = name.into();
        let layout = ServiceLayout::for_product(name.as_str());
        Self {
            description: name.clone(),
            exec_start: exec_start.into(),
            working_directory: layout.install_root().to_path_buf(),
            environment_file: Some(layout.env_file()),
            environment: Vec::new(),
            user: None,
            group: None,
            restart: "always".to_string(),
            restart_sec: 5,
            memory_high: None,
            memory_max: None,
            cpu_quota: None,
            tasks_max: None,
            limit_nofile: None,
            name,
        }
    }

    pub fn for_layout(layout: &ServiceLayout, binary_name: &str) -> Self {
        Self::new(layout.product(), layout.bin_dir().join(binary_name))
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

    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("[Unit]\n");
        output.push_str(&format!("Description={}\n", self.description));
        output.push_str("After=network.target\n\n");
        output.push_str("[Service]\n");
        output.push_str("Type=simple\n");
        if let Some(user) = &self.user { output.push_str(&format!("User={user}\n")); }
        if let Some(group) = &self.group { output.push_str(&format!("Group={group}\n")); }
        if let Some(environment_file) = &self.environment_file {
            output.push_str(&format!("EnvironmentFile={}\n", environment_file.display()));
        }
        for (key, value) in &self.environment {
            output.push_str(&format!("Environment={key}={value}\n"));
        }
        output.push_str(&format!("ExecStart={}\n", self.exec_start.display()));
        output.push_str(&format!("WorkingDirectory={}\n", self.working_directory.display()));
        output.push_str(&format!("Restart={}\n", self.restart));
        output.push_str(&format!("RestartSec={}\n", self.restart_sec));
        if let Some(memory_high) = &self.memory_high { output.push_str(&format!("MemoryHigh={memory_high}\n")); }
        if let Some(memory_max) = &self.memory_max { output.push_str(&format!("MemoryMax={memory_max}\n")); }
        if let Some(cpu_quota) = &self.cpu_quota { output.push_str(&format!("CPUQuota={cpu_quota}\n")); }
        if let Some(tasks_max) = self.tasks_max { output.push_str(&format!("TasksMax={tasks_max}\n")); }
        if let Some(limit_nofile) = self.limit_nofile { output.push_str(&format!("LimitNOFILE={limit_nofile}\n")); }
        output.push_str("\n[Install]\n");
        output.push_str("WantedBy=multi-user.target\n");
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{ServiceLayout, SystemdService};
    use std::path::PathBuf;

    #[test]
    fn layout_uses_opt_product_root() {
        let layout = ServiceLayout::for_product("rustzen-admin");
        assert_eq!(layout.install_root().to_path_buf(), PathBuf::from("/opt/rustzen-admin"));
        assert_eq!(layout.env_file(), PathBuf::from("/opt/rustzen-admin/config/app.env"));
    }

    #[test]
    fn systemd_render_has_required_fields() {
        let layout = ServiceLayout::for_product("rustzen-admin");
        let service = SystemdService::for_layout(&layout, "rustzen-admin").render();
        assert!(service.contains("EnvironmentFile=/opt/rustzen-admin/config/app.env"));
        assert!(service.contains("ExecStart=/opt/rustzen-admin/bin/rustzen-admin"));
        assert!(service.contains("Restart=always"));
    }
}
