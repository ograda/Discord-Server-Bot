# Discord Server Bot

Bot privado para administrar 2–3 servidores próprios, com código-fonte público.
Monólito modular em Rust, desenvolvido no Windows e preparado para VPS Linux.
O objetivo é evoluir administração, automações e integrações incrementalmente.

## V1.1

Anúncios por formulário, regras em embeds, catálogo de links e prévias privadas.
Consulte o [guia V1.1](docs/v1.1.md) para migração, comandos completos, templates,
persistência JSON, recuperação e homologação.
O registro central, defaults compartilhados e comandos independentes estão no
[guia multiguild](docs/multiguild.md). TOMLs são carregados somente na inicialização.

| Comando | Função | Permissão do usuário |
|---|---|---|
| `/ping` | Confirma disponibilidade | Servidor autorizado |
| `/status` | Versão, estado e uptime | Servidor autorizado |
| `/announce channel content` | Publica texto no canal configurado | Gerenciar mensagens |
| `/role add member role` | Adiciona cargo | Gerenciar cargos |
| `/role remove member role` | Remove cargo | Gerenciar cargos |
| `/announcements create/edit/list/delete` | Gerencia anúncios com prévia | Gerenciar mensagens |
| `/rules preview/publish/update/edit/add/remove/list` | Gerencia regras | Gerenciar mensagens |
| `/links preview/publish/update/edit/add/remove/list` | Gerencia links | Gerenciar mensagens |

Comandos administrativos exigem também cargo moderador configurado, quando a lista
não estiver vazia. O dono do servidor dispensa essas restrições pessoais.
O bot verifica suas próprias permissões, permissões do canal e hierarquias.
Anúncios não geram menções a usuários, cargos, `@everyone` ou `@here`.
Respostas dos comandos são efêmeras. Não há comandos prefixados.

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
3. Mantenha desabilitadas todas as **Privileged Gateway Intents**. O código usa
   apenas `GUILDS`; não necessita Message Content, Presence ou Server Members.
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

## Configuracao

`config/default.toml` concentra configuracoes globais e `[guilds.alias]` com `id`,
`name` e `enabled`. `name` determina diretamente a pasta `config/guilds/<name>`.
Nao repita identidade nos arquivos locais. Nomes com espacos e Unicode sao aceitos;
nomes reservados, caminhos e colisoes equivalentes sao rejeitados.

Os arquivos locais padronizados sao `config.toml`, `commands.toml`, `rules.toml`,
`announcements.toml` e `links.toml`. Eles herdam os arquivos de mesmo nome em
`config/guilds/defaults/`. Propriedades/mapas mesclam individualmente; listas sao
substituidas, e listas vazias limpam o conteudo. Arquivos locais podem faltar.

`commands.toml` habilita cada comando ou subcomando. Modulos desabilitados bloqueiam
seus comandos; o registro slash omite comandos desligados e grupos vazios.
Reinicie apos alterar TOMLs. Personalizacoes JSON feitas no Discord continuam
prioritarias. O estado permanece em `data/guilds/<Guild ID>/state.json`.

Para migrar uma instalacao V1.1, pare o bot e execute:

```sh
cargo run --bin migrate-config -- config templates
cargo run --bin migrate-config -- --check config
```

Fontes e backup sao preservados. Veja [o guia completo](docs/multiguild.md).

| Variavel | Uso |
|---|---|
| `DISCORD_TOKEN` | Token privado obrigatorio |
| `CONFIG_DIR` | Configuracoes, padrao `config` |
| `DATA_DIR` | Estado JSON, padrao `data` |
| `RUST_LOG` | Sobrescreve o logging global |
| `TEMPLATE_DIR` | Somente compatibilidade com templates legados |

Nunca publique `.env`, dados JSON ou configuracoes reais. Pastas locais sao
ignoradas, mas **`config/default.toml` ja e rastreado pelo Git**: revise o staging
e nao inclua o registro privado em commits. Use `default.toml.example` como exemplo
publico. O build Docker inclui somente fontes e manifestos Cargo.

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
portas expostas. Configuração é montada somente leitura; logs vão para stdout.
O contêiner recebe SIGTERM e encerra conexões. A imagem final não contém toolchain.
Conteúdos TOML ficam em `config`, montado somente para leitura. O volume `bot-data` mantém
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

Em um servidor de desenvolvimento, valide manualmente: os cinco comandos,
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
