# CPU Guardian

Controlador de carga e freio de mão emergencial de CPU em Rust para Linux.

## Propósito do Projeto

O **Cpu Guardian** foi projetado para evitar que sistemas Linux sob cargas pesadas (como compilações paralelas, scripts desgovernados ou renderizações) entrem em colapso, sofram travamento total de interface gráfica, engasgos de áudio (*audio stuttering*) ou congelamento do mouse.

Diferente do `nice` tradicional (que não resolve processos multi-thread em loop), o Cpu Guardian atua de forma cirúrgica e reversível:

1. **Confinamento a 1 núcleo (`sched_setaffinity`)**: Isola o processo em 1 única CPU lógica, liberando imediatamente todos os outros núcleos para o sistema operacional, compositor gráfico e tarefas do usuário.
2. **Despriorização via CFS (`SCHED_BATCH` / `SCHED_IDLE`)**: Instrui o escalonador a tratar o processo como lote em segundo plano.
3. **Freio Total Emergencial (`SIGSTOP` / `SIGCONT`)**: Pausa o processo ofensor instantaneamente se o modo `handbrake` estiver ativo, restabelecendo com `SIGCONT` ao normalizar a carga.
4. **Perfis de Hardware Integrados (`profiles/`)**: Inclui perfis sob medida para CPUs com restrições térmicas e de núcleos, como o Intel Core i5-2450M (Sandy Bridge).
5. **Serviço Persistente (`systemd --user`)**: Roda em segundo plano contínuo, iniciando junto com a sessão do usuário.

## Estrutura do Repositório

```text
├── Cargo.toml                  # Dependências e metadados Rust
├── .gitignore                  # Arquivos ignorados pelo Git (target/, etc)
├── README.md                   # Documentação detalhada em português
├── AGENTS.md                   # Diretrizes para agentes e desenvolvedores
├── install.sh                  # Script de instalação do serviço systemd
├── systemd/
│   └── cpu-guardian.service    # Definição do serviço de usuário systemd
├── profiles/
│   ├── default.toml            # Perfil equilibrado genérico para Linux
│   └── i5-2450m.toml           # Perfil otimizado para Intel i5-2450M
├── config.example.toml         # Exemplo completo de arquivo de configuração
└── src/
    ├── main.rs                 # Loop do orquestrador, sinais (ctrl-c) e CLI
    ├── governor.rs             # Máquina de estados com histerese anti-flapping
    ├── config.rs               # Parser de configuração e resolução de perfis
    └── platform.rs             # Chamadas de sistema Linux (sched, affinity, signals)
```

## Regras de Desenvolvimento e Manutenção

- **Linux-First**: Manter foco estrito no ecossistema Linux moderno (`nix`, `libc`, `cgroups`/`sched`).
- **Segurança Absoluta (Reversibilidade)**: Qualquer intervenção feita em um processo (`SCHED_BATCH`, afinidade ou sinal de pausa) DEVE ser 100% restaurada ao estado original quando a carga normalizar ou quando o programa for encerrado via `SIGINT`/`SIGTERM`.
- **Lista de Exclusão (`exclude`)**: Nunca aplicar intervenções nos processos vitais do sistema operacional (`systemd`, `dbus`, `Xorg`, `wayland`, compositores, drivers de áudio `pipewire`/`wireplumber` e o próprio `cpu-guardian`).
- **Não Poluir Repositório**: Arquivos de build (`target/`) e arquivos locais de configuração (`config.toml`) devem permanecer no `.gitignore`.
