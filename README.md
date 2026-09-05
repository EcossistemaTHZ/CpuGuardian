# CPU Guardian (Linux)

Controlador de carga e freio de mão de emergência em Rust para evitar colapso e congelamento do sistema operacional Linux sob cargas pesadas sustentadas de CPU.

> Este programa atua de forma preventiva e emergencial para preservar a responsividade da máquina (mouse, janelas, áudio e terminais), sem matar processos e de forma **100% reversível**.

## Como Funciona o "Freio de Mão"

Diferente do simples ajuste de `nice` (que não impede processos pesados com múltiplas threads de travar o desktop), o Cpu Guardian emprega intervenções reais no escalonador e na afinidade do Linux:

1. **Confinamento de Núcleos (`sched_setaffinity`)**:
   - Confinará o processo ofensor a **1 único núcleo** de CPU (o último disponível), liberando imediatamente os demais núcleos para o sistema operacional, áudio e interface gráfica.
2. **Despriorização via Escalonador (`SCHED_BATCH` / `SCHED_IDLE`)**:
   - Aplica política `SCHED_BATCH` (ou `SCHED_IDLE` se executado como root), instruindo o Completely Fair Scheduler (CFS) do Linux a penalizar o processo em favor de processos interativos.
3. **Freio Total de Emergência (`SIGSTOP` / `SIGCONT`)**:
   - No modo `action = "handbrake"`, além do confinamento e escalonador, envia o sinal `SIGSTOP` imediatamente quando o sistema atinge o limiar crítico, estancando a CPU a 0% no mesmo instante. Ao normalizar ou no encerramento, envia `SIGCONT`.
4. **100% Reversível**:
   - Salva a máscara de afinidade e a política de escalonamento original.
   - Restaura o estado anterior quando a carga estabiliza ou ao pressionar `Ctrl+C`.

---

## Modos de Ação

Configuráveis no `config.toml` através do campo `action`:

- `throttle` (Recomendado): Confinamento estrito a 1 núcleo + política `SCHED_BATCH`/`SCHED_IDLE`. O processo continua executando, mas não monopoliza a máquina.
- `handbrake`: Freio total de emergência. Aplica throttle e congela o processo com `SIGSTOP` até o sistema esfriar/desafogar.

---

## Compilação e Uso

### 1. Compilar

```bash
cargo build --release
```

O binário otimizado estará em `target/release/cpu-guardian`.

### 2. Uso com Perfis Pré-definidos (`profiles/`)

O Cpu Guardian suporta perfis pré-configurados na pasta `profiles/`. Você pode listar os perfis disponíveis:

```bash
./target/release/cpu-guardian --list-profiles
```

E rodar diretamente passando o nome do perfil (ex: `i5-2450m`):

```bash
./target/release/cpu-guardian i5-2450m
```

Ele procurará automaticamente por `profiles/i5-2450m.toml`, `profiles/i5-2450m` ou por caminhos diretos. Se nenhum argumento for passado, usará `config.toml` ou `profiles/default.toml`.

### 3. Inicialização Automática com a Máquina (Serviço Systemd)

Para deixar o Cpu Guardian rodando em segundo plano de forma contínua, iniciando automaticamente com seu login no Linux:

```bash
./install.sh i5-2450m
```

O script:
- Compila o binário otimizado e instala em `~/.local/bin/cpu-guardian`.
- Ativa a configuração com o perfil desejado em `~/.config/cpu-guardian/config.toml`.
- Habilita e inicia a unidade systemd do usuário (`cpu-guardian.service`).

#### Comandos úteis do serviço:

```bash
# Ver status e consumo
systemctl --user status cpu-guardian

# Acompanhar logs das intervenções em tempo real
journalctl --user -u cpu-guardian -f

# Parar temporariamente ou reiniciar
systemctl --user stop cpu-guardian
systemctl --user restart cpu-guardian
```

---

## Lista de Proteção (`exclude`)

O Cpu Guardian vem com uma lista de exclusão para nunca interferir em componentes vitais do Linux:
- `systemd`, `init`, `kthreadd`, `dbus-daemon`, `dbus-broker`
- Servidores gráficos e compositores: `Xorg`, `wayland`, `gnome-shell`, `kwin`, `sway`, `hyprland`, `mutter`
- Servidores de som: `pipewire`, `wireplumber`, `pulseaudio`
- Shells e redes: `sshd`, `NetworkManager`, `bash`, `zsh`, `fish`, `cpu-guardian`
