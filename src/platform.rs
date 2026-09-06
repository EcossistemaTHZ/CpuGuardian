use anyhow::{bail, Context, Result};
use nix::{
    libc,
    sched::{sched_getaffinity, sched_setaffinity, CpuSet},
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use std::mem::MaybeUninit;

#[derive(Debug, Clone)]
pub struct OriginalState {
    pub policy: libc::c_int,
    pub priority: libc::c_int,
    pub affinity: Vec<usize>,
    pub is_frozen: bool,
}

/// Freia o processo ofensor:
/// 1. Coleta o estado de agendamento e afinidade original.
/// 2. Converte a política do processo para SCHED_BATCH (ou SCHED_IDLE como root).
/// 3. Opcionalmente reduz a afinidade por percentual, preservando paralelismo.
/// 4. Opcionalmente, aplica o "freio total" congelando com SIGSTOP imediatamente.
pub fn throttle(pid: u32, cpu_percent: u8, freeze: bool) -> Result<OriginalState> {
    let nix_pid = Pid::from_raw(pid as i32);

    // 1. Obter política e prioridade atuais
    let mut param = MaybeUninit::<libc::sched_param>::zeroed();
    let policy = unsafe { libc::sched_getscheduler(pid as libc::pid_t) };
    if policy < 0 {
        bail!(
            "não foi possível ler política de agendamento do PID {pid}: {}",
            std::io::Error::last_os_error()
        );
    }

    if unsafe { libc::sched_getparam(pid as libc::pid_t, param.as_mut_ptr()) } < 0 {
        bail!(
            "não foi possível ler sched_param do PID {pid}: {}",
            std::io::Error::last_os_error()
        );
    }
    let param = unsafe { param.assume_init() };

    // 2. Obter máscara de afinidade atual
    let current_cpuset = sched_getaffinity(nix_pid)
        .with_context(|| format!("falha ao ler afinidade do PID {pid}"))?;

    let mut original_affinity = Vec::new();
    let total_cpus = CpuSet::count();
    for cpu in 0..total_cpus {
        if current_cpuset.is_set(cpu).unwrap_or(false) {
            original_affinity.push(cpu);
        }
    }

    // 3. Aplicar política de agendamento em segundo plano
    // No Linux:
    // - SCHED_BATCH instrui o CFS a penalizar o processo em favor de processos interativos,
    //   e pode ser revertido livremente para SCHED_OTHER sem necessitar de root/CAP_SYS_NICE.
    // - SCHED_IDLE é ainda mais agressivo, mas sua reversão posterior requer CAP_SYS_NICE/root.
    let is_root = unsafe { libc::geteuid() == 0 };
    let target_policy = if is_root {
        libc::SCHED_IDLE
    } else {
        libc::SCHED_BATCH
    };
    let sched_param = libc::sched_param { sched_priority: 0 };
    if unsafe { libc::sched_setscheduler(pid as libc::pid_t, target_policy, &sched_param) } < 0 {
        bail!(
            "falha ao alterar política do PID {pid}: {}",
            std::io::Error::last_os_error()
        );
    }

    // 4. Limite progressivo. 100% preserva todo o paralelismo e aplica apenas SCHED_BATCH.
    if cpu_percent < 100 {
        apply_affinity_percent(nix_pid, &original_affinity, cpu_percent)?;
    }

    // 5. Freio de mão emergencial (SIGSTOP) se solicitado
    let mut is_frozen = false;
    if freeze {
        if let Ok(()) = kill(nix_pid, Signal::SIGSTOP) {
            is_frozen = true;
        }
    }

    Ok(OriginalState {
        policy,
        priority: param.sched_priority,
        affinity: original_affinity,
        is_frozen,
    })
}

fn apply_affinity_percent(pid: Pid, original: &[usize], cpu_percent: u8) -> Result<()> {
    if original.is_empty() {
        bail!("máscara de afinidade vazia para PID {}", pid);
    }
    let keep = ((original.len() * cpu_percent as usize) + 99) / 100;
    let keep = keep.clamp(1, original.len());
    let mut cpuset = CpuSet::new();
    // Mantém as CPUs de índice mais alto e deixa CPU0 livre para kernel/interface.
    for &cpu in original.iter().rev().take(keep) {
        cpuset
            .set(cpu)
            .with_context(|| format!("CPU lógica {cpu} inválida"))?;
    }
    sched_setaffinity(pid, &cpuset)
        .with_context(|| format!("falha ao aplicar afinidade progressiva ao PID {}", pid))
}

pub fn limit_affinity(pid: u32, state: &OriginalState, cpu_percent: u8) -> Result<()> {
    apply_affinity_percent(Pid::from_raw(pid as i32), &state.affinity, cpu_percent)
}

/// Restaura o processo ao estado original anterior ao throttle
pub fn restore(pid: u32, state: &OriginalState) -> Result<()> {
    let nix_pid = Pid::from_raw(pid as i32);

    // Se estiver congelado com SIGSTOP, descongela imediatamente
    if state.is_frozen {
        let _ = kill(nix_pid, Signal::SIGCONT);
    }

    // Restaura afinidade original
    if !state.affinity.is_empty() {
        let mut cpuset = CpuSet::new();
        for &cpu in &state.affinity {
            let _ = cpuset.set(cpu);
        }
        sched_setaffinity(nix_pid, &cpuset)
            .with_context(|| format!("falha ao restaurar afinidade do PID {pid}"))?;
    }

    // Restaura política de escalonamento original
    let param = libc::sched_param {
        sched_priority: state.priority,
    };
    let result = unsafe {
        // Se a política anterior era SCHED_OTHER (0) ou SCHED_BATCH (3) ou SCHED_IDLE (5)
        let target_policy = if state.policy < 0 {
            libc::SCHED_OTHER
        } else {
            state.policy
        };
        libc::sched_setscheduler(pid as libc::pid_t, target_policy, &param)
    };
    if result < 0 {
        bail!(
            "falha ao restaurar política do PID {pid}: {}",
            std::io::Error::last_os_error()
        );
    }

    Ok(())
}

/// Congela temporariamente um processo já monitorado
#[allow(dead_code)]
pub fn freeze(pid: u32) -> Result<()> {
    let nix_pid = Pid::from_raw(pid as i32);
    kill(nix_pid, Signal::SIGSTOP).with_context(|| format!("falha ao enviar SIGSTOP para {pid}"))
}

/// Descongela temporariamente um processo pausado
#[allow(dead_code)]
pub fn unfreeze(pid: u32) -> Result<()> {
    let nix_pid = Pid::from_raw(pid as i32);
    kill(nix_pid, Signal::SIGCONT).with_context(|| format!("falha ao enviar SIGCONT para {pid}"))
}
