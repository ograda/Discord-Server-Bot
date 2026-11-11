# Validação V1.1.0 — 11/11/2026

Ambiente: Windows, Rust/Cargo 1.98.1. Homologação Discord pendente.

| Comando executado | Resultado |
|---|---|
| `cargo fmt --all -- --check` | Passou |
| `cargo clippy --all-targets -- -D warnings` | Passou sem avisos |
| `cargo test` | 21 testes passaram |
| `cargo build` | Passou |
| `docker compose config --quiet` | Passou, código de saída 0 |
| `docker compose build` | Falhou no gerenciador de credenciais Docker local |
| `git diff --check` | Passou |
| `git fetch origin` | Bloqueado: sem escrita em `.git/FETCH_HEAD` |
| `git add ...` | Bloqueado: sem escrita em `.git/index.lock` |

O Docker também informou acesso negado à configuração local e ausência do plugin
buildx. A imagem Linux não foi construída nesta sessão. Não houve login no Discord,
publicação de mensagens, teste de modals em conta real ou execução do workflow remoto.

Os testes cobrem configuração antiga, múltiplos servidores, aliases, templates e
sobrescritas, isolamento, limites de conteúdo, URLs, menções, layout das dez regras,
registro de slash commands, permissões por canal, hierarquia, persistência após
reinício, substituição de snapshots, arquivo inválido, lock entre processos e
gravações concorrentes. Erros de integração com as APIs Rust foram corrigidos
durante a compilação. A fixture antiga foi corrigida para não depender do nível de
logging escolhido localmente pelo operador.

A revisão não encontrou segredos nos arquivos da entrega. `.env`, configurações
reais, templates privados por guild e `data/` foram confirmados como ignorados pelo
Git. A alteração prévia em `config/default.toml` foi preservada e não está incluída
na lista de staging abaixo. As alterações prévias de tratamento de erro em
`src/core/bot.rs` foram preservadas e integradas com os novos comandos.

## Git

Remoto: `https://github.com/ograda/Discord-Server-Bot`.
Branch local: `main`. HEAD anterior: `1a1129e` (`first commit discord bot`).
Nenhum novo commit ou push foi realizado: este ambiente só permite leitura de `.git`.
Não foi possível atualizar referências remotas para verificar avanços no servidor.

Execute no PowerShell do proprietário do repositório, na raiz do projeto:

```powershell
git status
git remote -v
git fetch origin
git log --oneline --left-right HEAD...origin/main
```

Se houver novos commits remotos, integre-os preservando o histórico e suas
alterações locais antes da publicação. Não use force push. Depois de revisar:

```powershell
git add .env.example .gitignore Cargo.toml Cargo.lock Dockerfile compose.yaml README.md docs src tests config/guilds/v1.1.toml.example templates/global templates/guilds/override.toml.example
git diff --cached --check
git diff --cached --stat
git diff --cached
git commit -m "feat: add multiguild messaging, rules and links management"
git push origin main
```

Não use `git add .` sem revisar as configurações locais. A publicação no Discord
deve seguir o roteiro de [homologação V1.1](v1.1.md#homologação-no-discord).
