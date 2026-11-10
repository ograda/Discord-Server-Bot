# Discord Server Bot

Bot privado para administrar 2–3 servidores próprios, com código-fonte público.
Monólito modular em Rust, desenvolvido no Windows e preparado para VPS Linux.
O objetivo é evoluir administração, automações e integrações incrementalmente.

## V1.0

| Comando | Função | Permissão do usuário |
|---|---|---|
| `/ping` | Confirma disponibilidade | Servidor autorizado |
| `/status` | Versão, estado e uptime | Servidor autorizado |
| `/announce channel content` | Publica texto no canal configurado | Gerenciar mensagens |
| `/role add member role` | Adiciona cargo | Gerenciar cargos |
| `/role remove member role` | Remove cargo | Gerenciar cargos |

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
Copy-Item config/guilds/example.toml.example config/guilds/servidor.toml
```

No Linux:

```sh
cp .env.example .env
cp config/guilds/example.toml.example config/guilds/servidor.toml
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
5. Conceda ao bot **Ver canais**, **Enviar mensagens**, **Gerenciar mensagens** e
   **Gerenciar cargos**. Não conceda Administrador. Gerenciar mensagens é exigido
   pela política administrativa dos anúncios, mesmo que enviar texto não o exija.
6. Posicione o cargo do bot acima dos cargos e membros que ele deverá gerenciar.
   Configure sobrescritas de canal permitindo visualizar e enviar anúncios.

Ative o Modo Desenvolvedor no Discord para copiar IDs de servidor, canais e cargos.
Os comandos são registrados somente nos servidores listados nos TOMLs, na conexão
inicial; não há registro global. Reinicie após mudanças na configuração. Remover
um servidor do TOML bloqueia suas interações mesmo se comandos antigos ainda
aparecerem no cliente; remova a aplicação desse servidor para concluir a retirada.

## Configuração

`config/default.toml` contém `app_name`, `log_level` e os padrões `[modules]`.
Use níveis `trace`, `debug`, `info`, `warn`, `error` ou `off`.
Cada arquivo ativo `config/guilds/*.toml` representa um servidor autorizado.
O exemplo completo está em `config/guilds/example.toml.example`.

- `guild_id`: ID obrigatório, preferencialmente escrito como string decimal.
- `[modules]`: bloco opcional com administration, messaging e roles; se ausente,
  herda todo o bloco global. Se presente, informe os três valores booleanos.
- `[channels].announcements`: único canal permitido para anúncios; obrigatório
  quando messaging estiver ativo. Aceita canal de texto ou de anúncios.
- `[channels].moderation_logs`: reservado para evolução; ainda não recebe logs.
- `[roles].moderators`: lista adicional de cargos autorizados; vazia significa
  verificar somente a permissão Discord. Não concede permissões por si só.
- `[messaging].enabled`: permite desabilitar mensagens especificamente.

IDs inválidos, campos desconhecidos, servidores duplicados, erros TOML e ausência
de servidores ativos interrompem o início com uma mensagem contextual. Exemplos
`.example` são ignorados. Configurações ficam em memória e não têm hot reload.

| Variável | Uso |
|---|---|
| `DISCORD_TOKEN` | Obrigatória; token privado do bot |
| `CONFIG_DIR` | Diretório de configuração, padrão `config` |
| `RUST_LOG` | Sobrescreve nível global; aceita filtros do tracing |

Nunca versione `.env`, tokens ou arquivos reais dos servidores. O `.gitignore`
ignora esses arquivos e bancos locais; o contexto Docker inclui somente fontes e
manifestos Cargo. Nunca cole o token em comandos que ficam no histórico.

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
