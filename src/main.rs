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

    // Verifica status do ZRAM no Linux para informar ao usuário
    let zram_active = std::path::Path::new("/dev/zram0").exists();
    let zram_status = if zram_active { "ativo ✔" } else { "não detectado (considere ./install.sh --with-zram)" };

    println!(
        "CPU Guardian iniciado | dry_run={} | ação={:?} | intervalo={}ms | ZRAM={}",
        cfg.dry_run, cfg.action, cfg.sample_interval_ms, zram_status
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
        system.refresh_memory();

        let cpu = system.global_cpu_info().cpu_usage();
        let total_mem = system.total_memory();
        let avail_mem = system.available_memory();
        let avail_mem_pct = if total_mem > 0 {
            (avail_mem as f32 / total_mem as f32) * 100.0
        } else {
            100.0
        };

        let mem_critical = avail_mem_pct <= cfg.min_available_memory_percent;

        // Limpa processos que já morreram por conta própria
        managed.retain(|pid, _| system.process(Pid::from_u32(*pid)).is_some());

        let mem_alert = if mem_critical { " [RAM CRÍTICA!]" } else { "" };
        println!(
            "CPU: {:>5.1}% | RAM Disp: {:>4.1}%{} | processos freados: {}",
            cpu, avail_mem_pct, mem_alert, managed.len()
        );

        let decision = governor.update(
            cpu,
            mem_critical,
            cfg.high_cpu_percent,
            cfg.panic_cpu_percent,
            cfg.low_cpu_percent,
            cfg.high_samples_before_action,
            cfg.panic_samples_before_action,
            cfg.low_samples_before_restore,
        );

        match decision {
            Decision::Throttle => {
                let freeze = cfg.action == Action::Handbrake;
                throttle_candidates(&system, &cfg, &mut managed, freeze);
            }
            Decision::PanicFreeze => {
                println!("🔥 SITUAÇÃO DE PÂNICO / QUASE COLAPSO! Acionando Handbrake (SIGSTOP) em todos os processos monitorados...");
                escalate_to_freeze(&cfg, &mut managed);
                // Também inclui novos candidatos ofensores se ainda houver slots
                throttle_candidates(&system, &cfg, &mut managed, true);
            }
            Decision::Restore => {
                restore_all(&cfg, &mut managed);
            }
            Decision::Hold => {}
        }
    }

    restore_all(&cfg, &mut managed);
    println!("CPU Guardian encerrado; todos os processos foram restaurados ao estado original.");
    Ok(())
}

fn throttle_candidates(
    system: &System,
    cfg: &Config,
    managed: &mut HashMap<u32, OriginalState>,
    freeze: bool,
) {
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

    for (pid, process) in candidates.into_iter().take(cfg.max_managed_processes.saturating_sub(managed.len())) {
        let action_name = if freeze {
            "FREIO TOTAL (SCHED_IDLE + 1 Core + SIGSTOP)"
        } else {
            "THROTTLE (SCHED_IDLE + 1 Core)"
        };

        println!(
            "🚨 ALTA CARGA: {} (PID {}, CPU {:.1}%) -> Aplicando {}",
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

fn escalate_to_freeze(cfg: &Config, managed: &mut HashMap<u32, OriginalState>) {
    if cfg.dry_run {
        return;
    }
    for (pid, state) in managed.iter_mut() {
        if !state.is_frozen {
            if let Ok(()) = platform::freeze(*pid) {
                state.is_frozen = true;
                println!("   ⏸ PID {} pausado via SIGSTOP para alívio imediato do sistema.", pid);
            }
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
