#!/usr/bin/env bash
set -e

PROFILE=${1:-"i5-2450m"}

echo "==> Compilando o CPU Guardian em modo release..."
cargo build --release

echo "==> Instalando binário em ~/.local/bin/..."
mkdir -p ~/.local/bin
install -m 755 target/release/cpu-guardian ~/.local/bin/cpu-guardian

echo "==> Configurando diretório ~/.config/cpu-guardian/..."
mkdir -p ~/.config/cpu-guardian

if [ -f "profiles/${PROFILE}.toml" ]; then
    echo "==> Copiando perfil 'profiles/${PROFILE}.toml' para ~/.config/cpu-guardian/config.toml..."
    cp "profiles/${PROFILE}.toml" ~/.config/cpu-guardian/config.toml
elif [ -f "$PROFILE" ]; then
    echo "==> Copiando '$PROFILE' para ~/.config/cpu-guardian/config.toml..."
    cp "$PROFILE" ~/.config/cpu-guardian/config.toml
else
    echo "==> Perfil '$PROFILE' não encontrado em profiles/. Usando default..."
    cp profiles/default.toml ~/.config/cpu-guardian/config.toml
fi

echo "==> Instalando serviço do systemd do usuário..."
mkdir -p ~/.config/systemd/user
cp systemd/cpu-guardian.service ~/.config/systemd/user/cpu-guardian.service

echo "==> Recarregando systemd e habilitando serviço..."
systemctl --user daemon-reload
systemctl --user enable --now cpu-guardian.service

# --- Otimização de ZRAM (Memória Comprimida + Anti-Disk Thrashing) ---
setup_zram() {
    echo ""
    echo "==> Verificando otimização ZRAM (Memória Comprimida)..."
    
    if ! command -v zramctl >/dev/null 2>&1; then
        echo "--> Pacote zram-tools não encontrado. Instalando..."
        if command -v sudo >/dev/null 2>&1; then
            sudo apt-get update -qq && sudo apt-get install -y zram-tools
        else
            echo "--> Aviso: sudo indisponível para instalar zram-tools automaticamente."
            return
        fi
    fi

    echo "--> Otimizando configuração do ZRAM (/etc/default/zramswap)..."
    if command -v sudo >/dev/null 2>&1; then
        sudo tee /etc/default/zramswap >/dev/null << 'EOF'
ALGO=zstd
PERCENT=100
PRIORITY=100
EOF

        echo "--> Aplicando parâmetros de kernel ideais (/etc/sysctl.d/99-zram.conf)..."
        sudo tee /etc/sysctl.d/99-zram.conf >/dev/null << 'EOF'
vm.swappiness=180
vm.page-cluster=0
vm.vfs_cache_pressure=50
EOF
        sudo sysctl --system >/dev/null 2>&1 || true

        echo "--> Reiniciando serviço zramswap..."
        sudo systemctl stop zramswap >/dev/null 2>&1 || true
        sudo systemctl start zramswap >/dev/null 2>&1 || true
        echo "✔ ZRAM otimizado e ativo!"
    else
        echo "--> Aviso: sem permissões de sudo para ajustar /etc/default/zramswap."
    fi
}

# --- Sentinela Anti-Travamento (EarlyOOM) ---
setup_earlyoom() {
    echo ""
    echo "==> Verificando sentinela EarlyOOM..."
    if ! command -v earlyoom >/dev/null 2>&1; then
        echo "--> Instalando earlyoom para proteção final contra congelamento de memória..."
        if command -v sudo >/dev/null 2>&1; then
            sudo apt-get update -qq && sudo apt-get install -y earlyoom
        fi
    fi
    if command -v systemctl >/dev/null 2>&1 && command -v sudo >/dev/null 2>&1; then
        sudo systemctl enable --now earlyoom >/dev/null 2>&1 || true
        echo "✔ EarlyOOM ativado como serviço!"
    fi
}

# Pergunta ou executa o setup de ZRAM e EarlyOOM se solicitado ou se o usuário tiver sudo
if [ "$INSTALL_ZRAM" = "true" ] || [ "$2" = "--with-zram" ] || [ "$1" = "--with-zram" ]; then
    setup_zram
    setup_earlyoom
else
    # Se já temos sudo sem senha ou se o usuário quiser, configura zram
    if sudo -n true 2>/dev/null; then
        setup_zram
        setup_earlyoom
    else
        echo ""
        echo "==> Dica: para instalar/otimizar ZRAM e EarlyOOM automaticamente com o CPU Guardian, rode:"
        echo "    ./install.sh $PROFILE --with-zram"
    fi
fi

echo ""
echo "=========================================================="
echo "✔ CPU Guardian instalado e ativado com sucesso!"
echo "  Perfil ativo : $PROFILE"
echo "  Binário      : ~/.local/bin/cpu-guardian"
echo "  Config       : ~/.config/cpu-guardian/config.toml"
echo "  Status       : systemctl --user status cpu-guardian"
echo "  Logs em tempo real: journalctl --user -u cpu-guardian -f"
if command -v zramctl >/dev/null 2>&1; then
    echo "----------------------------------------------------------"
    echo "  Status ZRAM  : $(zramctl 2>/dev/null | tail -n 1)"
fi
echo "=========================================================="
