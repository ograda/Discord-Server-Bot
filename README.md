# Discord Server Bot

Bot privado para administrar 2–3 servidores próprios, com código-fonte público.
Monólito modular em Rust, desenvolvido no Windows e preparado para VPS Linux.
O objetivo é evoluir administração, automações e integrações incrementalmente.

## V1.1 — interface simplificada

Oito comandos raiz, com escolhas nativas em `ação`. `/role` e `/role-all` exigem
ação explícita; os demais comandos com ação têm um padrão. Não há subcomandos
nativos. As respostas administrativas são efêmeras.

| Comando | Sem selecionar ação | Outras operações |
|---|---|---|
| `/ping` | Pong | `modo:status` (versão, uptime, conexão e execução) |
| `/role` | Ação obrigatória | `ação:add/remove`, `usuário` USER e `cargo` ROLE obrigatórios |
| `/role-all` | Ação obrigatória, com confirmação | `ação:add/remove`, `cargo` ROLE; humanos elegíveis |
| `/rules` | Prévia privada | `ação:publish`, `update`, `reset`, `remove` |
| `/links` | Prévia do quadro; menu se houver vários | `ação:publish`, `update`, `reset`, `add`, `remove` |
| `/announcement` | Criar anúncio por mensagem ou formulário | `ação:edit`, `remove`; seleção/autocomplete |
| `/announce` | Envia embed para announcements | `mensagem` obrigatória, `canal` opcional |
| `/profile` | Confirma restauração do perfil TOML | `ação:nickname`, `bio`, `avatar` |

**Update recarrega TOML em memória; Publish exige confirmação para modificar o
Discord.** Reset confirma, preserva backup do arquivo específico e restaura o
arquivo compartilhado, sem publicar. Links Add grava no TOML e atualiza a memória.
Remove apaga somente a publicação selecionada; os TOMLs continuam disponíveis.

Rules e Links usam TOML como fonte de conteúdo. Documentos antigos do JSON são
preservados em backup e arquivados antes da descontinuação dessa precedência.
Message IDs e referências operacionais continuam no JSON por Guild ID.

Comandos administrativos exigem Gerenciar Mensagens; Profile exige Gerenciar
Servidor; Role exige Gerenciar Cargos e Role-all exige também Gerenciar Servidor.
Também é exigido o cargo moderador
configurado, se houver. O dono da guilda dispensa essas restrições pessoais.
As permissões do bot e a hierarquia continuam obrigatórias. O usuário autorizado
não precisa de permissão direta de escrita no canal de anúncios. Menções são
suprimidas. O perfil global e outras guildas não são alterados.

Veja o [guia de comandos](docs/commands.md), o [guia de cargos individuais e em massa](docs/roles.md), [V1.1](docs/v1.1.md),
[arquitetura](docs/architecture.md) e [multiguild](docs/multiguild.md).

## Stack e estrutura

Rust, Poise 0.6 (Serenity 0.12 via reexport), Tokio, Serde/TOML, anyhow,
dotenvy, tracing/tracing-subscriber e Docker Compose. `Cargo.lock` fixa a resolução.
Sem banco de dados, áudio, scheduler, painel web ou microsserviços nesta versão.

```text
src/main.rs          ambiente, configuração e logs
src/config/          tipos, carregamento e validação TOML
src/core/            cliente, comandos básicos e autorização
src/modules/         anúncios e cargos
src/services/        conteúdo, prévias, publicação e armazenamento JSON
config/guilds/defaults/ padrões compartilhados de configuração e conteúdo
config/              padrões globais e exemplos por servidor
tests/               testes de configuração sem Discord
docs/                arquitetura e roadmap
.github/workflows/   CI Linux
```

## Requisitos e instalação

Use Rust estável atualizado (validado com 1.98.1), Git e, opcionalmente,
Docker Desktop com WSL2 no Windows ou Docker Engine + Compose v2 no Linux.
O build nativo Windows requer as ferramentas C++ do Visual Studio Build Tools.

```sh
git clone https://github.com/ograda/Discord-Server-Bot.git
cd Discord-Server-Bot
cargo fetch --locked
```

No PowerShell:

```powershell
Copy-Item .env.example .env
# Instalação nova: não sobrescreva um default.toml privado existente.
Copy-Item config/default.toml.example config/default.toml
New-Item -ItemType Directory -Force 'config/guilds/O Acordo'
Copy-Item config/guilds/config.toml.example 'config/guilds/O Acordo/config.toml'
```

No Linux:

```sh
cp .env.example .env
# Instalação nova; preserve arquivos privados existentes.
cp config/default.toml.example config/default.toml
mkdir -p 'config/guilds/O Acordo'
cp config/guilds/config.toml.example 'config/guilds/O Acordo/config.toml'
chmod 600 .env
```

## Discord Developer Portal

1. Crie uma aplicação no [Developer Portal](https://discord.com/developers/applications)
   e configure seu bot. Desative **Public Bot**, mantendo a instalação privada.
2. Gere o token na seção Bot e coloque-o somente no `.env` local.
3. Para uso individual, mantenha as intents privilegiadas desabilitadas: `/role`
   não precisa de Server Members. Para `/role-all`, habilite **Server Members Intent**
   no Portal e `[intents] guild_members = true` no TOML global. Não habilite Presence
   ou Message Content; veja o [guia de cargos](docs/roles.md).
4. Configure instalação no servidor com scopes `bot` e `applications.commands`.
   Convide a aplicação apenas para seus servidores usando uma conta autorizada.
5. Conceda ao bot **Ver canais** e **Enviar mensagens** para anúncios, e
   **Gerenciar cargos** para operações de cargos. Não conceda Administrador.
   O autor continua precisando da autorização administrativa, mas não precisa
   poder escrever diretamente no canal de destino. O bot não precisa de
   **Gerenciar mensagens** para enviar anúncios.
6. Posicione o cargo do bot acima dos cargos e membros que ele deverá gerenciar.
   Configure sobrescritas de canal permitindo visualizar e enviar anúncios.

Ative o Modo Desenvolvedor no Discord para copiar IDs de servidor, canais e cargos.
Os comandos são registrados somente nos servidores listados nos TOMLs, na conexão
inicial; não há registro global. Reinicie após mudanças na configuração. Remover
um servidor do TOML bloqueia suas interações mesmo se comandos antigos ainda
aparecerem no cliente; remova a aplicação desse servidor para concluir a retirada.

## Configuração

`config/default.toml` registra cada guilda por alias, `id`, `name` e `enabled`.
O nome determina `config/guilds/<nome>`; não existe campo `folder`. Arquivos
específicos herdam `config/guilds/defaults/`. Mapas mesclam campos; arrays são
substituídos por inteiro. Arquivos locais podem faltar.

`commands.toml` controla raízes em `[commands]` e ações em `[actions.rules]`,
`[actions.links]`, etc. Módulos desabilitados bloqueiam suas operações. Cada guilda
recebe suas escolhas habilitadas, e a execução revalida a disponibilidade. Se a
ação padrão estiver desabilitada, selecione outra explicitamente.

As configurações gerais são carregadas na inicialização. Rules e Links oferecem
Update explícito; Profile Revert lê seus defaults TOML novamente. Alterações em
módulos, comandos, canais e registro exigem reinicialização.

Exemplos públicos: [commands](config/guilds/commands.toml.example),
[rules](config/guilds/rules.toml.example), [links](config/guilds/links.toml.example)
e [announcements](config/guilds/announcements.toml.example).
URLs no TOML precisam de aspas: `url = "https://discord.gg/convite"`.
O formato legado `title` + `[[entries]]` de links continua aceito como quadro
`default`; o formato novo usa `[[boards]]` e `[[boards.links]]`.

Para migrar o cadastro legado, pare o bot e execute:

```sh
cargo run --bin migrate-config -- config templates
cargo run --bin migrate-config -- --check config
```

Não execute migração em uma instalação já registrada. Fontes e backup são
preservados. A migração de conteúdo JSON é automática na primeira inicialização
da nova versão, antes do registro de comandos. Não apague `state.json`.

| Variável | Uso |
|---|---|
| `DISCORD_TOKEN` | Token privado obrigatório |
| `CONFIG_DIR` | Configurações, padrão `config` |
| `DATA_DIR` | Estado JSON, padrão `data` |
| `RUST_LOG` | Sobrescreve o logging global |
| `TEMPLATE_DIR` | Compatibilidade/migração de templates legados |

Nunca publique `.env`, estados JSON ou configurações reais. Pastas específicas são
ignoradas; `config/default.toml` já é rastreado: não inclua alterações privadas no
staging. O build Docker copia somente fontes e manifestos Cargo.

## Executar

Depois de editar `.env` e o TOML real:

```sh
cargo run --locked
```

Encerre com Ctrl+C. Para Docker:

```sh
docker compose config --quiet
docker compose up -d --build
docker compose logs -f bot
docker compose down
```

Use `--quiet` ao validar Compose para não imprimir o token interpolado. Não há
portas expostas. O registro e os defaults são montados somente leitura; logs vão para stdout.
O contêiner recebe SIGTERM e encerra conexões. A imagem final não contém toolchain.
Os diretórios específicos em `config/guilds` permitem escrita para Reset e Links Add;
`config/guilds/defaults` continua somente leitura. Crie as pastas das guildas
e conceda escrita ao UID 10001 nelas antes de subir o contêiner (veja o guia multiguild).
O volume `bot-data` mantém
as publicações entre reinicializações, com escrita pelo UID 10001. Não remova esse
volume ao atualizar; faça backup com o bot parado.

## Qualidade e homologação

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

CI executa esses checks em pushes e pull requests para `main`, sem deploy ou
publicação de imagens. Testes locais cobrem parsing, validação, duplicidade,
seleção de servidor e regras de autorização; não precisam de token.

Em um servidor de desenvolvimento, valide manualmente: os oito comandos,
anúncio em canal permitido e negado, menções sem notificação, usuário sem
permissão/cargo, cargo acima do bot, cargo gerenciado, módulo desabilitado,
servidor não autorizado, reconexão e encerramento. Teste com cargos descartáveis.
Falhas da API são respondidas sem revelar detalhes internos. Não existe teste
automatizado de conexão real sem credenciais.

## Implantação Linux

Clone o mesmo repositório na VPS, instale Docker/Compose, crie os arquivos locais
como acima e execute `docker compose up -d --build`. Garanta que o UID 10001 tenha
leitura dos TOMLs e acesso aos diretórios montados. Mantenha `.env` restrito ao
operador. Libere saída HTTPS/WSS para Discord e DNS; nenhuma entrada pública é
necessária. Para atualizar, use `git pull --ff-only` e reconstrua com Compose.
Faça backup seguro das configurações privadas; logs têm rotação configurada.

## Evolução

Veja [arquitetura](docs/architecture.md) e [roadmap](docs/roadmap.md).
V1 evoluirá a administração; V1.2 adicionará automação e SQLite quando necessário;
V2 planeja Lavalink independente, múltiplas identidades musicais e minijogos.
Essas funcionalidades futuras ainda não estão implementadas.

Nenhuma licença foi adicionada; a escolha cabe ao proprietário.

Referências: [Poise](https://docs.rs/poise/0.6.2/poise/) e
[Serenity](https://docs.rs/serenity/0.12.5/serenity/).
