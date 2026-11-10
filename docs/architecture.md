# Arquitetura

Um pacote Cargo, monólito modular e runtime Tokio. `main` carrega ambiente e TOML,
configura tracing e entrega a execução ao núcleo. Poise gerencia comandos slash
e Serenity mantém a conexão Gateway, incluindo reconexão e retomada de sessão.
Serenity é usado pelo reexport de Poise para evitar versões incompatíveis.

`config` carrega configurações uma vez, antes de conectar. Apenas arquivos `.toml`
diretamente em `guilds` são ativos. Campos desconhecidos, IDs inválidos, servidores
duplicados e lista vazia interrompem a inicialização. IDs usam os tipos Snowflake
de Serenity, sem conversão para ponto flutuante. `Data` centraliza a configuração
imutável e o início do tempo operacional (após registro dos comandos).

`core` mantém cliente, registro por servidor, comandos básicos e autorização.
`modules/messaging` publica anúncios; `modules/roles` altera cargos. Administração
é uma política central em `core/permissions`, sem um módulo vazio adicional.
Os módulos são habilitados por servidor; ausência de `[modules]` herda o bloco
global integralmente. Administração desabilitada bloqueia anúncios e cargos.

Cada interação verifica a lista de servidores autorizados, sem exceção para donos
da aplicação. A autorização administrativa consulta membros e cargos via HTTP:
permissão Discord obrigatória, mais cargo configurado quando houver restrição.
O dono do servidor dispensa essas restrições pessoais; o bot nunca dispensa suas
próprias permissões e hierarquia. Anúncios verificam sobrescritas do canal para
autor e bot. Alterações de cargos verificam hierarquias de autor, alvo e bot,
incluindo desempate de posição pelo ID implementado por Serenity. Cargos gerenciados,
`@everyone`, dono do servidor e o próprio bot são protegidos.

As permissões padrão dos slash commands facilitam a interface; as verificações
no servidor continuam obrigatórias. Como no Discord, mudanças concorrentes de
cargos/permissões entre consulta e ação ainda podem fazer a API rejeitar a ação.
Não são solicitadas intents privilegiadas: somente `GUILDS`. Membros individuais
são consultados via REST, sem baixar a lista de membros.

Logs estruturados vão para stdout, sem conteúdo de anúncios, argumentos ou token.
Respostas administrativas são efêmeras. Menções são desabilitadas nos anúncios.
Ctrl+C e SIGTERM encerram os shards. Docker executa usuário 10001, filesystem
somente leitura, sem portas públicas e com configuração montada somente leitura.

Sem banco, fila, scheduler ou abstração de áudio na V1. Para V1.2, SQLite/SQLx
armazenará estado dinâmico; TOML continuará sendo a configuração estática.
Na V2, Lavalink será outro serviço Compose: Rust controla comandos, permissões e
players. Várias identidades de bot poderão compartilhar esse serviço. Reprodução
simultânea em canais do mesmo servidor exige identidades Discord separadas, com
tokens próprios, a serem projetadas nessa versão.
