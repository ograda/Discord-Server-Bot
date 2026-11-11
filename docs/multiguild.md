# Configuração multiguild

Uma aplicação e um processo atendem todas as guildas. `config/default.toml` é o
único registro global. Não existe `servers.toml`, propriedade `folder` ou pasta
`config/defaults/`. Os TOMLs são carregados e validados uma vez na inicialização.
Nenhum comando relê configurações ou templates; alterações exigem reinício.

## Cadastro e estrutura

Preserve `app_name`, `log_level` e as configurações globais existentes. Cadastre:

```toml
[guilds.o-acordo]
id = "123456789012345678"
name = "O Acordo"
enabled = true
```

A chave é o alias interno, `id` é o Guild ID permanente e `name` é exatamente o
nome da pasta. `enabled` assume true se ausente. Não repita esses campos nos
arquivos locais. Consulte `config/default.toml.example` (IDs fictícios).

```text
config/
  default.toml
  guilds/
    defaults/
      config.toml
      commands.toml
      rules.toml
      announcements.toml
      links.toml
    O Acordo/
      config.toml       # somente diferenças; demais arquivos são opcionais
      commands.toml
      rules.toml
data/guilds/<Guild ID>/state.json
```

O nome da pasta pode ter espaços e Unicode. São rejeitados nomes vazios, caminhos,
separadores, caracteres de controle, caracteres proibidos no Windows, espaços ou
pontos finais, dispositivos reservados e `defaults`. Colisões são verificadas com
maiúsculas Unicode e normalização NFKC, conservadoramente também impedindo nomes
compatíveis equivalentes. O nome original nunca é corrigido ou substituído.
Guild IDs e aliases duplicados são rejeitados, inclusive em guildas desabilitadas.
Aliases mantêm a regra ASCII minúscula da V1.1.

Guildas desabilitadas permanecem no registro, mas não executam comandos. O registro
slash envia uma lista vazia para elas, removendo comandos antigos quando o bot ainda
tem acesso ao servidor. Guildas removidas completamente do registro são bloqueadas
em execução; remova a aplicação no Discord para retirar comandos antigos restantes.

## Configuração efetiva e canais

Ordem: propriedades internas/legadas globais → `guilds/defaults/config.toml` →
`guilds/<name>/config.toml`. Tabelas e mapas são mesclados recursivamente. Uma
propriedade ausente herda o padrão; um booleano false realmente desabilita.
Listas sempre são substituídas; `moderators = []` limpa a restrição adicional.

Exemplo local:

```toml
[modules]
rules = true
links = true
[channels]
rules = "222222222222222222"
links = "333333333333333333"
[channels.custom]
events = "444444444444444444"
```

O Guild ID da interação seleciona a configuração em memória. `config.guild(alias)`
resolve a guilda e `config.channel(alias, canal)` resolve o canal. Aliases de canais
são locais, portanto dois servidores podem ter `rules` com IDs diferentes.
Operações administrativas sempre utilizam a guilda da interação; não recebem um
alias de outra guilda como destino. As permissões e hierarquias V1.1 permanecem.

## Comandos por servidor

`commands.toml` tem somente a tabela `[commands]`. Padrões compartilhados são
mesclados por entrada com o arquivo local. Entradas ausentes em ambos ficam true,
mas nunca habilitam um módulo desabilitado. Chaves desconhecidas são erro.

```toml
[commands]
announce = false
"rules.publish" = false
"rules.update" = true
"role.remove" = false
```

Use nomes completos: `ping`, `status`, `announce`, `role.add`, `role.remove` e
os subcomandos de `announcements`, `rules` e `links` listados nos defaults.
Não use uma chave de grupo como `rules = false`; desligue o módulo ou suas entradas.

O mesmo predicado controla registro e execução. Grupos parcialmente habilitados
contêm somente os subcomandos permitidos; grupos vazios são omitidos. Interações
antigas também são bloqueadas. Confirmações de conteúdo verificam adicionalmente
`rules.publish`/`links.publish` ou `rules.update`/`links.update`, conforme exista
publicação: editar um rascunho não contorna uma publicação desabilitada.

## Conteúdos

Os arquivos `rules.toml`, `announcements.toml` e `links.toml` usam o modelo Document
da V1.1: title, description, color, thumbnail, image, footer, text e entries.
Cada entrada contém id, title, description, emoji, url e order. Em regras,
description é a introdução. Imagens/thumbnail são URLs HTTP/HTTPS opcionais.

Prioridade: documento JSON persistido da guilda → conteúdo TOML específico →
conteúdo compartilhado em `guilds/defaults/` → padrão interno mínimo.
Campos ausentes herdam. `entries = []` limpa a lista; uma lista específica substitui
a lista inteira, nunca concatena. Não copie arquivos que não precisam de diferenças.
Exemplo de `rules.toml` local que mantém as dez regras compartilhadas:

```toml
title = "📜 Regras da nossa comunidade"
description = "Bem-vindo! Leia nossas orientações."
```

Para regras completamente próprias, forneça `[[entries]]` para cada regra. Links
seguem o mesmo princípio. Anúncios podem sobrescrever apenas cor/rodapé/título.
Conteúdos editados pelo Discord continuam sendo snapshots JSON prioritários e não
são sobrescritos por mudanças no TOML nem reinícios.

## Persistência preservada

A V1.1 existente usa **um `state.json` por Guild ID**, reunindo documents,
publications, revision e pending. Esse formato foi preservado, em vez de dividir
em publications.json/rules.json/links.json e perder a transação atômica existente.
Nenhuma mudança de dados é necessária. Renomear a pasta e o campo `name` não altera
o diretório de estado, IDs de mensagens, rascunhos ou registros de reconciliação.
Locks, gravações atômicas e proteção contra duplicação permanecem.

## Migração efetiva da V1.1

Pare o bot. Preserve os templates legados até concluir a migração. Execute:

```sh
cargo run --bin migrate-config -- config templates
cargo run --bin migrate-config -- --check config
```

O utilitário lê todos os arquivos `config/guilds/*.toml`, valida IDs, aliases,
nomes, conflitos e conteúdo, monta e valida uma árvore temporária, cria arquivos
locais e substitui `default.toml` atomicamente por último. Cria um backup em
`default.toml.pre-multiguild`; nunca sobrescreve fontes ou destinos já existentes.
Campos globais existentes são preservados semanticamente; a formatação/comentários
do TOML global podem mudar. Nome ausente no legado usa o nome do arquivo sem extensão;
alias ausente usa `guild-<id>`. Valores inválidos são rejeitados, sem sanitização.

Configurações efetivas de módulos e templates legados são preservadas. Conteúdos
iguais ao default não geram cópias redundantes. Dados JSON não são lidos nem escritos
pelo migrador. Não existe chamada à API Discord. Se uma escrita falhar, o backup e
as fontes continuam disponíveis: revise os destinos parciais antes de tentar novamente.

Compatibilidade temporária: **ausência** da tabela `[guilds]` ativa exclusivamente
o carregador antigo e seus templates legados, também carregados uma vez no início.
Presença de `[guilds]`, mesmo vazia, torna o registro central autoritativo: arquivos
soltos legados são ignorados, não carregados duas vezes. Para acrescentar uma guilda
após migrar, registre-a no global; criar só um arquivo legado não a autoriza.

Nesta entrega uma guilda local foi efetivamente migrada. O legado e três templates
originais foram comparados por SHA-256 e permaneceram idênticos. Nenhum JSON
operacional existente foi alterado. O backup global contém dados privados e é ignorado.

## Docker e Git

Compose agora monta somente `config/` (leitura) e o volume persistente `data/`
(escrita pelo UID 10001). Não necessita do antigo volume `templates/` após migração.
Para instalações Docker ainda legadas, execute primeiro o migrador nativamente
ou mantenha temporariamente o volume antigo até migrar.

Pastas privadas em `config/guilds/<name>/` são ignoradas pelo Git; defaults e exemplos
são públicos. **`config/default.toml` já era rastreado e agora contém o registro real**:
não o inclua em commits públicos. Revise staging explicitamente e use
`config/default.toml.example` como referência pública. Não use `git add .` indiscriminadamente.

## Expansão e homologação

Novos módulos poderão adicionar arquivos padronizados no diretório de defaults e
no diretório de cada guilda, com tipo Serde e carregamento no startup. Não foram
adicionados calendar, feeds, music ou games.

No Discord, teste duas guildas: canais diferentes para o mesmo alias, regras
distintas, um subcomando desabilitado só em uma delas e um módulo desabilitado.
Reinicie e confirme o registro slash filtrado. Teste também comandos antigos,
rascunho com publicação bloqueada, atualização preservando ID e renomeação de pasta
sem perda de estado. Nenhuma integração Discord real foi executada nesta sessão.
# Perfil opcional e diagnóstico de comandos

Para habilitar o perfil, configure `[modules] profile = true` no `config.toml`
da guilda. Em `commands.toml`, as entradas `profile.show`, `profile.nickname`,
`profile.avatar`, `profile.bio` e `profile.apply` controlam cada subcomando.
O módulo permanece desabilitado por padrão para preservar as configurações antigas.

`[profile]` aceita `nickname`, `bio` e `avatar`; este último usa um caminho como
`assets/avatar.png`, relativo à pasta da guilda. Overrides compartilhados seguem
a mesma mesclagem dos demais campos, mas o asset é sempre resolvido dentro da
guilda de destino. Personalizações por comando ficam em `data/guilds/<ID>/state.json`.
Não há alteração do TOML nem propagação de alterações para outras guildas.

Um módulo desabilitado omite seus comandos mesmo quando `commands.toml` os habilita.
O startup registra os comandos efetivos e motivos de omissão por guilda. Execute
`cargo run --bin migrate-config -- --check` para diagnosticar a configuração sem
conectar ao Discord. Reinicie o bot para registrar alterações no Discord.
Veja [exemplos e checklist](stabilization-profile.md).
