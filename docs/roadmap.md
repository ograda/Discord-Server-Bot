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

## V2.0 — Components V2 e Activity (em desenvolvimento)

Primeira entrega na branch `feat/v2-components-activity`, versão `2.0.0-dev.1`:

- Links em Container nativo, com textos, thumbnail e botões internos.
- Adaptador REST tipado isolado, mantendo Serenity/Poise e demais módulos V1.
- Conversão confirmada por PATCH, preservando Message ID e estados existentes.
- Activity estática TypeScript/Vite/Embedded App SDK com openInviteDialog.
- LAUNCH_ACTIVITY por botão, configuração opt-in independente por guilda.
- Testes locais, build frontend e roteiro de homologação em servidor elegível.

A V1 funcional permanece no histórico. Não há release 2.0 estável nem validação
real da Activity em servidores grandes. Veja [restrições e preparação](activity-invite.md).

### Evoluções futuras, fora desta entrega

Lavalink, música, múltiplas identidades musicais, scheduler e minijogos continuam
planejados para etapas posteriores. Nenhum desses serviços ou bancos foi adicionado
à primeira entrega da V2.
