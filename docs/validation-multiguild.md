# Validação da refatoração multiguild — 11/11/2026

Implementação local preserva V1.1.0, sem release ou nova funcionalidade administrativa.

## Migração realizada

- Identificado um arquivo real legado em `config/guilds/servidor.toml`.
- Executado `cargo run --bin migrate-config -- config templates`: uma guilda migrada.
- Registro central atualizado e configuração efetiva criada em sua pasta de nome.
- Fonte legada e três templates originais preservados; SHA-256 idêntico antes/depois.
- Backup em `config/default.toml.pre-multiguild` criado, sem sobrescrever arquivos.
- `cargo run --bin migrate-config -- --check config`: uma guilda válida e habilitada.
- Nenhum JSON operacional alterado ou mensagem Discord modificada.
- Snapshot `state.json` mantido para preservar atomicidade e dados V1.1 existentes.

## Verificações

| Comando | Resultado |
|---|---|
| `cargo fmt --all -- --check` | Passou |
| `cargo clippy --all-targets -- -D warnings` | Passou |
| `cargo test` | 29 testes passaram |
| `cargo build` | Passou |
| `docker compose config --quiet` | Passou, saída 0 |
| `docker compose build` | Falhou no acesso ao gerenciador de credenciais Docker |
| `git diff --check` | Passou |

Foi corrigido um panic no primeiro teste do carregador novo: indexação mutável de
chave inexistente em `toml::Value` foi substituída por inserção explícita na tabela.
A normalização Unicode usa ICU4X já presente nas dependências transitivas, com
somente os recursos necessários ativados. Nenhuma nova biblioteca foi baixada.

Testes novos cobrem registro, nomes portáveis/Unicode, colisões, configurações
herdadas, substituição de listas, independência, cache de conteúdo, grupos slash
vazios/parciais, comandos desconhecidos, guildas desabilitadas, renomeação preservando
conteúdo, migração e recusa de destinos preexistentes. A suíte V1.1 foi preservada.
Fixtures de teste não incluem mais `config/default.toml` privado em tempo de compilação.

Docker também informou acesso negado à configuração local e ausência de buildx.
A imagem Linux não foi validada. Não houve conexão Discord ou execução da CI remota.
Homologação: seguir [Multiguild](multiguild.md#expansão-e-homologação).

## Git

Remoto confirmado: `https://github.com/ograda/Discord-Server-Bot`; branch `main`.
Commit não criado: staging bloqueado por `.git/index.lock: Permission denied`.
Push não realizado. Nenhuma permissão foi alterada e nenhum histórico foi reescrito.
As alterações V1.1 anteriores continuam presentes.

No terminal do proprietário, revise e publique (sem incluir o registro privado):

```powershell
git status
git fetch origin
git log --oneline --left-right HEAD...origin/main
# Integre eventuais commits remotos preservando as alterações locais.
git add .env.example .gitignore Cargo.toml Cargo.lock Dockerfile compose.yaml README.md docs src tests config/default.toml.example config/guilds/defaults config/guilds/config.toml.example config/guilds/commands.toml.example config/guilds/v1.1.toml.example templates/global templates/guilds/override.toml.example
git diff --cached --check
git diff --cached --stat
git diff --cached
git commit -m "refactor: reorganize multiguild configuration and defaults"
git push origin main
```

Os templates antigos na lista são fontes V1.1 preservadas, não defaults ativos do
novo formato. Não adicione `.env`, `data/`, pastas privadas, backup nem
`config/default.toml` com IDs reais. Esse último já era rastreado e exige revisão
explícita de staging; `.gitignore` não desrastreia arquivos existentes.
