use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dry_run: bool,
    pub sample_interval_ms: u64,
    pub high_cpu_percent: f32,
    pub low_cpu_percent: f32,
    pub high_samples_before_action: u32,
    pub low_samples_before_restore: u32,
    pub minimum_process_cpu_percent: f32,
    pub max_managed_processes: usize,
    pub action: Action,
    pub exclude: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Despriorização severa (SCHED_IDLE) + Confinamento a 1 único núcleo de CPU
    Throttle,
    /// Freio de mão emergencial (SCHED_IDLE + 1 núcleo + congelamento imediato via SIGSTOP)
    Handbrake,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dry_run: true,
            sample_interval_ms: 1000,
            high_cpu_percent: 80.0,
            low_cpu_percent: 60.0,
            high_samples_before_action: 3,
            low_samples_before_restore: 4,
            minimum_process_cpu_percent: 15.0,
            max_managed_processes: 2,
            action: Action::Throttle,
            exclude: vec![
                // Serviços essenciais do sistema e init
                "systemd", "systemd-journald", "systemd-udevd", "kthreadd", "init",
                // Barramentos e rede
                "dbus-daemon", "dbus-broker", "sshd", "NetworkManager",
                // Servidores de display e compositores (evitar congelar a interface gráfica)
                "Xorg", "wayland", "gnome-shell", "kwin_wayland", "kwin_x11",
                "sway", "hyprland", "mutter",
                // Áudio e multimídia (evitar engasgos de som)
                "pipewire", "pipewire-pulse", "wireplumber", "pulseaudio",
                // Shells e o próprio CpuGuardian
                "bash", "zsh", "fish", "cpu-guardian",
            ].into_iter().map(str::to_owned).collect(),
        }
    }
}

impl Config {
    /// Resolve o caminho da configuração. Se não for um arquivo direto, procura na pasta `profiles/`
    /// por `<nome>.toml` ou `<nome>`.
    pub fn resolve_path(arg: &str) -> PathBuf {
        let direct_path = PathBuf::from(arg);
        if direct_path.is_file() {
            return direct_path;
        }

        // Tenta na pasta profiles/
        let profile_dir = Path::new("profiles");
        let profile_exact = profile_dir.join(arg);
        if profile_exact.is_file() {
            return profile_exact;
        }

        let with_toml = profile_dir.join(format!("{}.toml", arg));
        if with_toml.is_file() {
            return with_toml;
        }

        // Retorna o caminho original para exibir erro claro caso não exista
        direct_path
    }

    /// Lista os perfis disponíveis na pasta profiles/
    pub fn list_available_profiles() -> Vec<String> {
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir("profiles") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        profiles.push(stem.to_string());
                    }
                }
            }
        }
        profiles.sort();
        profiles
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| {
                let available = Self::list_available_profiles();
                let hint = if !available.is_empty() {
                    format!("\nPerfis disponíveis na pasta profiles/: {}", available.join(", "))
                } else {
                    String::new()
                };
                format!("não foi possível ler {}{hint}", path.display())
            })?;
        let cfg: Self = toml::from_str(&text).context("configuração TOML inválida")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.low_cpu_percent < self.high_cpu_percent,
            "low_cpu_percent deve ser menor que high_cpu_percent");
        anyhow::ensure!((100..=60_000).contains(&self.sample_interval_ms),
            "sample_interval_ms deve estar entre 100 e 60000");
        anyhow::ensure!(self.high_cpu_percent <= 100.0 && self.low_cpu_percent >= 0.0,
            "limites de CPU devem estar entre 0 e 100");
        anyhow::ensure!(self.max_managed_processes > 0,
            "max_managed_processes deve ser maior que zero");
        Ok(())
    }

    pub fn is_excluded(&self, name: &str) -> bool {
        self.exclude.iter().any(|x| x.eq_ignore_ascii_case(name))
    }
}
