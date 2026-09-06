# CPU Guardian (Linux)

Controlador de carga e freio de mão de emergência em Rust para evitar colapso e congelamento do sistema operacional Linux sob cargas pesadas sustentadas de CPU.

> Este programa atua de forma preventiva e emergencial para preservar a responsividade da máquina (mouse, janelas, áudio e terminais), sem matar processos e de forma **100% reversível**.

## Como Funciona o "Freio de Mão"

Diferente de um limitador fixo, o Cpu Guardian combina carga, memória e temperatura real para aplicar intervenções progressivas:

1. **Controle progressivo (`sched_setaffinity`)**:
   - Carga alta comum apenas desprioriza o processo e preserva o paralelismo. A afinidade é reduzida somente quando o sensor térmico indica calor; o percentual é configurável.
2. **Despriorização via Escalonador (`SCHED_BATCH` / `SCHED_IDLE`)**:
   - Aplica política `SCHED_BATCH` (ou `SCHED_IDLE` se executado como root), instruindo o Completely Fair Scheduler (CFS) do Linux a penalizar o processo em favor de processos interativos.
3. **Temperatura real (`hwmon` / `thermal_zone`)**:
   - Lê `coretemp` em `/sys/class/hwmon` e usa `/sys/class/thermal` como fallback. Carga de 100% sozinha não é tratada como superaquecimento quando existe um sensor válido.
4. **Freio Total de Emergência (`SIGSTOP` / `SIGCONT`)**:
   - No modo dinâmico, `SIGSTOP` fica reservado para temperatura de emergência ou pressão crítica de memória. Ao normalizar ou no encerramento, envia `SIGCONT`.
5. **100% Reversível**:
   - Salva a máscara de afinidade e a política de escalonamento original.
   - Restaura o estado anterior quando a carga estabiliza ou ao pressionar `Ctrl+C`.

---

## Modos de Ação

Configuráveis no `config.toml` através do campo `action`:

- `throttle`: aplica `SCHED_BATCH`/`SCHED_IDLE` e o percentual configurado em `panic_cpu_percent_limit`.
- `handbrake`: Freio total de emergência. Aplica throttle e congela o processo com `SIGSTOP` até o sistema esfriar/desafogar.
- `nice_only`: aplica somente `SCHED_BATCH`/`SCHED_IDLE`, sem reduzir afinidade.
- `dynamic` (recomendado): preserva todos os CPUs lógicos sob carga comum, reduz afinidade quando há calor real e congela somente em emergência.

### Perfil i5-2450M

O perfil mantém as 4 threads durante uma compilação normal. Aos 82 °C começa a
proteção térmica e, aos 90 °C, mantém 75% das CPUs lógicas disponíveis (3 de 4).
O freio total fica reservado para 97 °C ou memória crítica. Confira se o sensor
está disponível no log (`Temp: N/D` indica que não foi encontrado):

```bash
sudo modprobe coretemp
cat /sys/class/hwmon/hwmon*/temp*_input
```

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

Para um Intel Core 2 Duo E8500:

```bash
./install.sh e8500
```

O perfil `e8500` mantém os dois núcleos durante carga comum e reduz a afinidade
para um núcleo somente quando a leitura térmica ultrapassa os limites configurados.

Ele procurará automaticamente por `profiles/i5-2450m.toml`, `profiles/i5-2450m` ou por caminhos diretos. Se nenhum argumento for passado, usará `config.toml` ou `profiles/default.toml`.

### 3. Inicialização Automática com a Máquina + Otimização ZRAM

Para deixar o **Cpu Guardian** rodando em segundo plano de forma contínua no boot e, de quebra, **ativar/otimizar o ZRAM automaticamente**:

```bash
./install.sh i5-2450m --with-zram
```

O script:
- Compila o binário otimizado e instala em `~/.local/bin/cpu-guardian`.
- Ativa a configuração com o perfil desejado em `~/.config/cpu-guardian/config.toml`.
- **Configura o ZRAM** (memória comprimida com `zstd`, prioridade 100 e `sysctl` anti-HDD-thrashing) caso não esteja instalado/otimizado.
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
