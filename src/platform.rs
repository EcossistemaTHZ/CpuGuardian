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
/// 2. Converte a política do processo para SCHED_IDLE (só consome se a CPU estiver 100% ociosa).
/// 3. Isola a afinidade para um único núcleo (ex: último core disponível), liberando o resto do sistema.
/// 4. Opcionalmente, aplica o "freio total" congelando com SIGSTOP imediatamente.
pub fn throttle(pid: u32, freeze: bool) -> Result<OriginalState> {
    let nix_pid = Pid::from_raw(pid as i32);

    // 1. Obter política e prioridade atuais
    let mut param = MaybeUninit::<libc::sched_param>::zeroed();
    let policy = unsafe { libc::sched_getscheduler(pid as libc::pid_t) };
    if policy < 0 {
        bail!("não foi possível ler política de agendamento do PID {pid}: {}", std::io::Error::last_os_error());
    }

    if unsafe { libc::sched_getparam(pid as libc::pid_t, param.as_mut_ptr()) } < 0 {
        bail!("não foi possível ler sched_param do PID {pid}: {}", std::io::Error::last_os_error());
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
    let target_policy = if is_root { libc::SCHED_IDLE } else { libc::SCHED_BATCH };
    let sched_param = libc::sched_param { sched_priority: 0 };
    unsafe {
        libc::sched_setscheduler(pid as libc::pid_t, target_policy, &sched_param);
    }

    // 4. Confinar o processo a apenas 1 núcleo para salvar a máquina
    // Escolhe o último core disponível para deixar cores 0..n livres para a interface gráfica e I/O
    let target_cpu = original_affinity.last().copied().unwrap_or(0);
    let mut restricted_cpuset = CpuSet::new();
    if restricted_cpuset.set(target_cpu).is_ok() {
        let _ = sched_setaffinity(nix_pid, &restricted_cpuset);
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
        let _ = sched_setaffinity(nix_pid, &cpuset);
    }

    // Restaura política de escalonamento original
    let param = libc::sched_param { sched_priority: state.priority };
    unsafe {
        // Se a política anterior era SCHED_OTHER (0) ou SCHED_BATCH (3) ou SCHED_IDLE (5)
        let target_policy = if state.policy < 0 { libc::SCHED_OTHER } else { state.policy };
        libc::sched_setscheduler(pid as libc::pid_t, target_policy, &param);
        libc::setpriority(libc::PRIO_PROCESS, pid, 0);
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
