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

echo ""
echo "=========================================================="
echo "✔ CPU Guardian instalado e ativado com sucesso!"
echo "  Perfil ativo : $PROFILE"
echo "  Binário      : ~/.local/bin/cpu-guardian"
echo "  Config       : ~/.config/cpu-guardian/config.toml"
echo "  Status       : systemctl --user status cpu-guardian"
echo "  Logs em tempo real: journalctl --user -u cpu-guardian -f"
echo "=========================================================="
