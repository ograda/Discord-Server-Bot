# Roadmap

## V1.0 — Administração

Base implementada: núcleo Rust/Poise/Serenity, TOML global e por servidor, allowlist,
`/ping`, `/status`, `/announce`, `/role add`, `/role remove`, autorização central,
logs, Docker, testes locais e CI. A homologação em um servidor Discord é manual.

Evolução V1: ampliar comandos administrativos conforme necessidades reais,
aprimorar auditoria e considerar envio de eventos ao canal `moderation_logs`
(hoje apenas reservado na configuração, sem envio). Não há hot reload.

## V1.1 — Messaging, Rules & Links

Correção aplicada: autorização do usuário separada das permissões operacionais do
bot. Anúncios dispensam MANAGE_MESSAGES do bot e envio direto pelo autor; cargos
mantêm a verificação explícita de MANAGE_ROLES e hierarquia.

Implementados: registro de aliases, templates globais e por servidor, serviço de
mensagens, modals e prévias privadas, anúncios gerenciados, regras, catálogo de
links, rascunhos, persistência JSON e volume Docker. Homologação real no Discord
segue o checklist em `docs/v1.1.md`. Não há Rules Screening próprio.

## V1.2 — Automação e integrações (planejado)

- Scheduler persistente, SQLite + SQLx, agendas e lembretes.
- Eventos nativos do Discord e mensagens programadas.
- Feeds RSS/Atom, APIs externas e controle de publicações duplicadas.

SQLite somente quando houver persistência dinâmica; configuração estática em TOML.

## V2.0 — Música e entretenimento (planejado)

- Lavalink Server em contêiner independente e cliente integrado ao Rust.
- Reprodução em voz, filas, playlists e controles interativos.
- Múltiplas identidades musicais e reprodução simultânea em diferentes canais
  de voz do mesmo servidor, usando identidades distintas.
- Minijogos, rankings e pontuações persistentes.

Rust continuará responsável por comandos, permissões e gerenciamento dos players.
Várias identidades poderão compartilhar o mesmo Lavalink. Nenhum componente de
áudio ou abstração antecipada faz parte da V1.
