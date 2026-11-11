# Arquitetura — V1.1 estabilizada

Um pacote Rust, Serenity 0.12 por reexport de Poise 0.6, runtime Tokio e armazenamento
JSON existente. Não há banco, scheduler, webhooks, painel ou serviço adicional.

## Componentes reutilizados

- `config`: registro multiguild, Serde/TOML, validação de nomes/IDs, defaults e flags.
- `core`: inicialização, registro slash por guilda, autorização e diagnósticos.
- `modules`: interface dos sete comandos; somente Role possui subcomandos.
- `services/content`: o mesmo construtor de embeds e validação de limites/URLs.
- `services/catalog`: snapshots mutáveis somente de Rules/Links, com recarga explícita.
- `services/publishing`: prévia/confirmar e transação de publicação da mensagem original.
- `services/announcements`: construtor de mensagem, chaves, seleção e links gerenciados.
- `services/profile`: download/decodificação, Data URI, REST por guilda e persistência.
- `services/storage`: lock entre processos, mutex, revisão, journal e snapshots JSON.
- `services/ui`: confirmações de uso único, menus paginados e abertura de formulários.

## Configuração e memória

`main` lê `.env` e o registro global. Configurações gerais ficam imutáveis em `Data`.
`name` do registro determina a pasta; o Guild ID determina o estado operacional.
Defaults e arquivos específicos mesclam tabelas; arrays são substituídos.

`Catalog` mantém um snapshot por Guild ID protegido por `RwLock`, com regras,
quadros de links, origem e revisão. Preview usa somente esse snapshot. Update lê
os arquivos, aplica fallback, valida e substitui o conteúdo somente após sucesso.
Uma falha mantém conteúdo e revisão anteriores. Não é um hot reload genérico:
configuração geral, módulos, canais e comandos continuam exigindo reinicialização.
Profile Revert lê novamente somente os defaults de perfil.

Reset valida o arquivo compartilhado antes de copiar seus bytes para a guilda.
Links Add usa `toml_edit` para preservar comentários e estrutura quando possível,
valida o arquivo efetivo resultante, cria backup exato e usa arquivo temporário
no mesmo diretório + rename atômico. Comentários em arrays inline normalizados
podem mudar; o backup mantém os bytes anteriores. O mutex de `Store` serializa
essas operações com as confirmações de publicação. Caminhos são resolvidos pelo
registro autorizado e não por parâmetros do usuário; escapes e symlinks/junções
nos caminhos de conteúdo são bloqueados.

## Comandos e autorização

As sete raízes são Ping, Role, Rules, Links, Announcement, Announce e Profile.
Choices opcionais permitem executar a raiz com a ação padrão. Parâmetros
condicionais são opcionais no esquema e validados por ação no runtime.
Somente Role mantém subcomandos reais; sua raiz não realiza operação.

Flags de raiz, flags de ação, módulo e status da guilda são considerados no
registro e na execução. Choices por guilda preservam seus índices originais do
Poise: filtrar renumerando os índices poderia executar a ação errada. O registro
usa `GuildId::set_commands`, bulk overwrite oficial apenas dos comandos da própria
aplicação naquela guilda, substituindo as definições públicas antigas.

A autorização administrativa consulta membros/cargos via REST: permissão exigida,
mais cargo moderador se configurado. O dono dispensa suas restrições pessoais.
O bot sempre verifica suas próprias permissões, hierarquia e acesso ao canal.
Não é exigida permissão direta de envio do autor no canal de anúncios.
Somente intent `GUILDS` é solicitada; nenhuma intent privilegiada.

## Publicação e estado

Rules e Links não leem conteúdo antigo do JSON. Antes do uso da nova versão,
o startup preserva o JSON original em `state.pre-toml-*.json.bak` e move os
documentos antigos para `archived_documents` no mesmo snapshot. Não perde
publicações, perfil ou informações de reconciliação. Migrações são idempotentes;
um journal pendente bloqueia uma migração que precisaria alterar aquele estado.

Cada quadro tem Board ID estável e uma publicação independente (`links:<id>`).
O quadro legado `default` preserva a chave `links`. Uma publicação legada pode ser
associada ao primeiro quadro da nova configuração durante a migração, mantendo
canal e Message ID e preservando o estado anterior em backup.

A confirmação é efêmera, vinculada ao autor, expira em 120 segundos e desativa
os botões antes dos efeitos. A autorização e revisões são conferidas novamente.
A transação busca a mensagem existente, verifica autoria e canal, grava `pending`,
faz POST/PATCH/DELETE e então salva a referência/revisão e limpa o journal.
PATCH preserva o Message ID. Uma confirmação repetida/obsoleta não repete o envio.
Erro de transporte mantém `pending` para evitar duplicação. Remove pode reconciliar
uma mensagem já ausente (404) após confirmação; Publish preserva os IDs nesse caso.

Anúncios gerenciados conservam seus documentos e chaves `announcement-<interaction>`.
O envio rápido usa o mesmo estilo de embed e não cria um anúncio gerenciado.
Menções são suprimidas em previews e publicações.

## Perfil e diagnósticos

Profile usa a API `PATCH /guilds/{id}/members/@me` com nick/bio/avatar e nunca
altera o perfil global. Revert representa valores ausentes como JSON null.
Customizações só são limpas após as requisições necessárias terem sucesso.
Falha no disco após sucesso remoto é explicada sem alegar uma transação distribuída.

Avatar é Attachment no slash, não upload em modal. O serviço limita download,
verifica MIME/assinatura, decodifica, gera Data URI e distingue cada etapa de erro.
Normalização para PNG 512×512 é explícita e proporcional. O hash retornado pela
API confirma a aplicação antes de persistir a personalização.

Logs incluem guilda, módulo, comando, ação, etapa e causa sanitizada. Erros HTTP
apresentam status/código quando disponíveis. Argumentos, tokens, corpos HTTP
livres e imagens Base64 não são registrados. Requisições de anexo não levam token.

## Execução

O processo encerra shards em Ctrl+C/SIGTERM. Docker mantém usuário 10001,
filesystem somente leitura, sem portas/capabilities e sem privilégios adicionais.
Apenas o volume de dados e a árvore de guildas permitem escrita; os defaults e o
registro continuam somente leitura. Veja permissões e backups em [multiguild](multiguild.md).
