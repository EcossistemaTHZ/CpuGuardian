mod config;
mod governor;
mod platform;

use anyhow::Result;
use config::{Action, Config};
use governor::{Decision, Governor};
use platform::OriginalState;
use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use sysinfo::{Pid, System};

fn main() -> Result<()> {
    let arg = env::args().nth(1);

    if let Some(ref a) = arg {
        if a == "--list-profiles" || a == "-l" {
            let profiles = Config::list_available_profiles();
            println!("Perfis disponíveis na pasta 'profiles/':");
            for p in profiles {
                println!("  - {p}");
            }
            return Ok(());
        }
    }

    let path = arg.as_deref().map(Config::resolve_path).unwrap_or_else(|| {
        if PathBuf::from("config.toml").is_file() {
            PathBuf::from("config.toml")
        } else if PathBuf::from("profiles/default.toml").is_file() {
            PathBuf::from("profiles/default.toml")
        } else {
            PathBuf::from("config.toml")
        }
    });

    let cfg = if path.exists() {
        println!("Carregando perfil/config: {}", path.display());
        Config::load(&path)?
    } else {
        let available = Config::list_available_profiles();
        eprintln!(
            "Config/Perfil '{}' não encontrado; usando padrões embutidos.\nPerfis disponíveis: {}",
            path.display(),
            if available.is_empty() { "nenhum".to_string() } else { available.join(", ") }
        );
        Config::default()
    };

    println!(
        "CPU Guardian iniciado | dry_run={} | ação={:?} | intervalo={}ms",
        cfg.dry_run, cfg.action, cfg.sample_interval_ms
    );

    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    ctrlc::set_handler(move || {
        println!("\nSinal de interrupção recebido, restaurando processos...");
        flag.store(false, Ordering::SeqCst);
    })?;

    let mut system = System::new_all();
    let mut governor = Governor::default();
    let mut managed: HashMap<u32, OriginalState> = HashMap::new();
    let interval = Duration::from_millis(cfg.sample_interval_ms);

    while running.load(Ordering::SeqCst) {
        thread::sleep(interval);
        system.refresh_cpu();
        system.refresh_processes();

        let cpu = system.global_cpu_info().cpu_usage();

        // Limpa processos que já morreram por conta própria
        managed.retain(|pid, _| system.process(Pid::from_u32(*pid)).is_some());

        println!(
            "CPU total: {:>5.1}% | processos freados: {}",
            cpu,
            managed.len()
        );

        match governor.update(
            cpu,
            cfg.high_cpu_percent,
            cfg.low_cpu_percent,
            cfg.high_samples_before_action,
            cfg.low_samples_before_restore,
        ) {
            Decision::Throttle => throttle_hottest(&system, &cfg, &mut managed),
            Decision::Restore => restore_all(&cfg, &mut managed),
            Decision::Hold => {}
        }
    }

    restore_all(&cfg, &mut managed);
    println!("CPU Guardian encerrado; todos os processos foram restaurados ao estado original.");
    Ok(())
}

fn throttle_hottest(system: &System, cfg: &Config, managed: &mut HashMap<u32, OriginalState>) {
    let own_pid = std::process::id();
    let mut candidates: Vec<_> = system
        .processes()
        .iter()
        .filter(|(pid, p)| {
            pid.as_u32() != own_pid
                && !managed.contains_key(&pid.as_u32())
                && p.cpu_usage() >= cfg.minimum_process_cpu_percent
                && !cfg.is_excluded(p.name())
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cpu_usage().total_cmp(&a.1.cpu_usage()));

    let freeze = cfg.action == Action::Handbrake;

    for (pid, process) in candidates.into_iter().take(cfg.max_managed_processes) {
        let action_name = if freeze {
            "FREIO TOTAL (SCHED_IDLE + 1 Core + SIGSTOP)"
        } else {
            "THROTTLE (SCHED_IDLE + 1 Core)"
        };

        println!(
            "🚨 ALTA CARGA DETECTADA: {} (PID {}, CPU {:.1}%) -> Aplicando {}",
            process.name(),
            pid,
            process.cpu_usage(),
            action_name
        );

        if cfg.dry_run {
            println!("   [SIMULAÇÃO] Nenhuma alteração feita no PID {}", pid);
            continue;
        }

        match platform::throttle(pid.as_u32(), freeze) {
            Ok(state) => {
                managed.insert(pid.as_u32(), state);
                println!("   ✓ PID {} freado com sucesso.", pid);
            }
            Err(e) => eprintln!("   ⚠ Aviso ao frear PID {}: {e}", pid),
        }
    }
}

fn restore_all(cfg: &Config, managed: &mut HashMap<u32, OriginalState>) {
    if managed.is_empty() {
        return;
    }

    println!("🔄 Restaurando {} processos gerenciados...", managed.len());
    if cfg.dry_run {
        managed.clear();
        return;
    }

    for (pid, state) in managed.drain() {
        match platform::restore(pid, &state) {
            Ok(()) => println!("   ✓ PID {} restaurado ao estado original.", pid),
            Err(e) => eprintln!("   ⚠ Aviso ao restaurar PID {}: {e}", pid),
        }
    }
}
