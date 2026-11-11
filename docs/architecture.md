# Arquitetura

Um pacote Cargo, monólito modular e runtime Tokio. `main` carrega ambiente e TOML,
configura tracing e entrega a execução ao núcleo. Poise gerencia comandos slash
e Serenity mantém a conexão Gateway, incluindo reconexão e retomada de sessão.
Serenity é usado pelo reexport de Poise para evitar versões incompatíveis.

`config` carrega configurações uma vez, antes de conectar. `default.toml` registra
as guildas por alias, ID, nome de pasta e habilitação. Arquivos padronizados em
`guilds/<name>` sobrescrevem `guilds/defaults`; tabelas mesclam recursivamente e
listas são substituídas integralmente. Campos desconhecidos, IDs inválidos e
nomes/aliases duplicados interrompem a inicialização. IDs usam os tipos Snowflake
de Serenity, sem conversão para ponto flutuante. `Data` centraliza a configuração
imutável e o início do tempo operacional (após registro dos comandos).

`core` mantém cliente, registro por servidor, comandos básicos e autorização.
`modules/messaging` publica anúncios; `modules/roles` altera cargos. Administração
é uma política central em `core/permissions`, sem um módulo vazio adicional.
Os módulos são habilitados por servidor com sobrescritas por propriedade.
Administração desabilitada bloqueia anúncios, regras, links e cargos.
`commands.toml` controla folhas individuais: a mesma política filtra o payload
slash de cada guilda e valida execução. Grupos vazios não são registrados.

Cada interação verifica a lista de servidores autorizados, sem exceção para donos
da aplicação. A autorização administrativa consulta membros e cargos via HTTP:
permissão Discord obrigatória, mais cargo configurado quando houver restrição.
O dono do servidor dispensa essas restrições pessoais; o bot nunca dispensa suas
próprias permissões e hierarquia. `administrative` verifica a autorização do autor;
as operações verificam separadamente as permissões do bot com
`ensure_bot_permissions` ou `ensure_bot_channel_permissions`. Anúncios verificam
sobrescritas do canal somente para o bot, sem exigir envio direto pelo autor.
Alterações de cargos verificam hierarquias de autor, alvo e bot,
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

## Conteúdo gerenciado V1.1

`config/registry` resolve aliases sem permitir que comandos escolham outros servidores.
`services/content` representa texto e embeds, valida limites e URLs e divide coleções
em cinco campos por embed. `services/publishing` aplica autorização, prévia privada,
confirmação vinculada ao autor, expiração e atualização de mensagens próprias.
`modules/content_commands` mantém os modals e os grupos announcements, rules e links.

`services/storage` usa um snapshot JSON por guild com documentos, publicações,
revisão e operação pendente. Gravação atômica em mesmo filesystem, mutex durante
transações e lock entre processos evitam perdas concorrentes. Revisões rejeitam
prévias obsoletas. O registro pendente bloqueia reenvio cego quando o resultado HTTP
é incerto; não há promessa de atomicidade distribuída Discord/disco.
Dados persistidos têm prioridade sobre conteúdos locais e compartilhados, carregados
em memória no startup. Não se alteram TOMLs pelo Discord. A identificação dos estados
continua por Guild ID, sem migração de JSON ou divisão do snapshot atômico.
ICU4X, já presente transitivamente, valida colisões de nomes por NFKC/maiúsculas.
O migrador valida destinos antes de gravar, preserva fontes, cria backup e substitui
o registro global por último. O registro central é autoritativo quando presente;
somente sua ausência ativa o carregador legado. Veja [Multiguild](multiguild.md).
# Extensão de perfil e diagnóstico V1.1

`services/announcements.rs` resolve chaves internas e Message IDs exclusivamente
contra publicações conhecidas da guilda. `services/profile.rs` contém validação de
assets, cálculo das diferenças e um adaptador REST tipado para o endpoint oficial
de membro atual. `modules/profile.rs` aplica a autorização administrativa existente.
O adaptador não altera o usuário global; sanitiza erros e respeita respostas 429.
Campos ausentes no Serenity são tratados apenas nesse adaptador, sem duplicar o
cliente de eventos. Imagens não aparecem em logs e são validadas antes do envio.

O campo opcional `profile` do estado existente mantém compatibilidade com JSONs
anteriores, usando o mesmo bloqueio e gravação atômica. Overrides JSON prevalecem
sobre TOML; assets TOML são carregados na inicialização e confinados à pasta da
guilda. Nenhuma atualização de perfil é executada automaticamente no startup.
Veja as limitações de leitura de biografia e a homologação no
[guia de estabilização](stabilization-profile.md).
