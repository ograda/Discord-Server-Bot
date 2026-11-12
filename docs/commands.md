# Guia de comandos — V1.1

Não há subcomandos reais. `/role`, `/role-all` e `/links` exigem `ação`; os comandos com
ação padrão usam uma escolha opcional `ação`. Ping utiliza `modo`. Não há `/status`, `/announcements`,
`/profile show`, `/profile apply` ou subcomandos nativos em Rules/Links.
A lista de choices é filtrada por guilda. Uma ação padrão desligada não é
substituída silenciosamente: selecione explicitamente outra permitida.

| Comando | Padrão | Choices |
|---|---|---|
| `/ping` | `simples` | `modo:simples`, `status` |
| `/role` | Nenhum; três parâmetros obrigatórios | `ação:add/remove`, `usuário` USER, `cargo` ROLE |
| `/role-all` | Nenhum; confirmação obrigatória | `ação:add/remove`, `cargo` ROLE; sem usuário |
| `/rules` | `preview` | `ação:preview`, `publish`, `update`, `reset`, `remove` |
| `/links` | Nenhum; ação obrigatória | `ação:preview`, `publish`, `update`, `reset`, `add`, `remove`; somente `id` opcional |
| `/announcement` | `create` | `ação:create`, `edit`, `remove` |
| `/announce` | Enviar embed | Não possui ação |
| `/profile` | `revert` | `ação:revert`, `nickname`, `bio`, `avatar` |

## Rules

`/rules` mostra os embeds atualmente em memória, em privado, sem botões de edição.
Todas as regras ficam em um único embed, com título e rodapé únicos, também na
publicação. Até 25 regras/campos e 6000 caracteres totais são aceitos; conteúdo
excessivo é rejeitado antes de publicar, sem truncar nem dividir em outros blocos.
Altere `config/guilds/<nome>/rules.toml` e execute `/rules ação:update` para validar
e recarregar. O bot informa o arquivo específico e os defaults usados; se houver
erro, preserva o snapshot anterior. Update não altera nenhuma mensagem pública.

`/rules ação:publish` mostra o conteúdo atual com apenas Publicar/Cancelar.
Confirmar cria a publicação, ou edita a original mantendo o Message ID. A sessão
expira em 120 segundos, é vinculada ao autor e é consumida uma única vez.
Mudanças de revisão invalidam a confirmação. Cancelar não salva nem publica.

`/rules ação:reset` confirma a cópia de `guilds/defaults/rules.toml` para a pasta
específica, preserva o arquivo anterior em `rules.toml.backup-*.bak`, valida antes
da gravação atômica e recarrega a memória. Não publica. `ação:remove` confirma a
exclusão da mensagem gerenciada e de sua referência; preserva a definição TOML.

## Links e quadros

Use [o exemplo](../config/guilds/links.toml.example) para definir `[[boards]]`.
Board ID identifica o quadro; `boards.links.id` identifica um link dentro dele.
`channel` é um alias de canal de `config.toml`, por exemplo `links` ou um canal de
`channels.custom`. IDs numéricos de canal, guilda e mensagem têm responsabilidades
separadas. Não mude o Board ID se deseja manter sua associação operacional.

`/links ação:preview` mostra o único quadro ou o quadro `default`; sem um padrão
inequívoco, abre um menu privado. O slash command possui somente `ação` obrigatória
e `id` opcional. ID é ignorado fora de Remove. Exemplos:

```text
/links ação:add
/links ação:remove id:whatsapp
```

Add abre, pelo botão privado Preencher formulário, o modal com ID, Título,
Descrição opcional, URL e Emoji opcional. Mostra prévia com Salvar/Cancelar e grava
somente no TOML específico, com backup e memória atualizada, sem publicar.
Remove oferece autocomplete/seleção dos links e confirmação; exclui somente a
entrada do TOML, preservando quadro, outras entradas e Message IDs.

Publish apresenta Editar/Publicar/Cancelar; confirmar cria ou atualiza a mesma
mensagem. Update recarrega o arquivo inteiro; Reset restaura defaults com backup.
O embed único apresenta links na mesma linha dos títulos, com `label` opcional
(padrão Abrir link). O convite `kind="discord_invite"`, também detectado pelas
URLs oficiais, aparece por último com botões de cópia manual privada e abertura
direta. Veja [formatação, persistência e homologação](links.md).

O formato antigo `title`, `description`, `[[entries]]` vira um quadro `default`.
Sua chave de publicação `links` continua válida. Ao adotar `[[boards]]`, o registro
antigo é associado ao primeiro quadro no startup, se essa associação ainda não
existir, com backup do estado anterior. Não há duplicação da mensagem.

## Announcement e envio rápido

`/announcement mensagem:"Aviso importante"` usa o estilo de `announcements.toml`,
mostra prévia e confirma antes de publicar. Sem `mensagem`, abre formulário de
título, descrição, cor e imagem principal opcional.

`/announcement ação:edit` e `ação:remove` oferecem seleção de anúncios gerenciados.
O parâmetro `identificador` tem autocomplete por título/chave/Message ID. Também
aceita o link de uma mensagem gerenciada da guilda atual. Edit aceita `mensagem`
ou formulário e mantém o Message ID após confirmação. Remove confirma exclusão.
Não existe operação pública List; o menu/autocomplete fornece a descoberta.

`/announce mensagem:"Aviso"` envia imediatamente um embed ao canal `announcements`.
`/announce mensagem:"Aviso" canal:#geral` usa outro canal da mesma guilda. Mantém
cor, título e imagem principal do template. Não cria um registro de anúncio gerenciado;
para edição/remoção pelo bot, utilize Announcement. Ambos suprimem menções.

Em envio rápido, criação, prévia e edição, o construtor de anúncios consulta a
guilda atual pela API: usa seu ícone como thumbnail, ou nenhuma thumbnail se não
houver ícone. O rodapé é sempre `Comunicado oficial • {guild.name} • Ograda Solutions Bot`.
Thumbnail e rodapé antigos do TOML/JSON são ignorados, sem reescrever os arquivos.
O nome e o ícone são consultados novamente ao publicar; a edição mantém o Message ID.

Ambos os comandos aceitam `imagem` (Attachment opcional) e `url_imagem`, como
alternativas exclusivas. Na versão Serenity 0.12.5, o upload fica no slash command;
o modal oferece URL alternativa, título, mensagem e cor, sem thumbnail/footer.
Em Announcement, `imagem_acao:manter/substituir/remover` torna a edição explícita.
Sem nova fonte, a imagem é mantida; URL vazia no formulário não remove a anterior.
Uploads são anexados à prévia privada e à mensagem pública somente após confirmar.
Detalhes de formatos, limites, persistência e testes em [Announcements](announcements.md).

## Profile

`/profile` ou `ação:revert` pede confirmação e aplica nickname/avatar/bio definidos
em `config.toml`, com fallback por campo para os defaults compartilhados. Valores
ausentes são removidos do perfil da guilda; nunca muda o username/avatar global.
As personalizações JSON são limpas somente após sucesso da operação remota.

```text
/profile ação:nickname apelido:"Bot da comunidade"
/profile ação:bio biografia:"Informações da comunidade"
/profile ação:avatar imagem:<anexo>
/profile ação:avatar imagem:<anexo> normalizar:true
```

Apelido/bio ausentes abrem formulário. Avatar exige um Attachment no slash command;
não há upload em modal nesta versão do Serenity. PNG/JPEG/GIF são reconhecidos
pelos bytes e decodificados; MIME, limite local de 4 MiB e dimensões até 4096×4096
são verificados. Uma imagem válida de 1254×1254 não é rejeitada pela resolução.
`normalizar:true` converte explicitamente para PNG 512×512 com proporção preservada
e margens transparentes. GIF normalizado fica estático. Sem a opção, não redimensiona.

O bot distingue falhas de validação, download, processamento e HTTP. Respostas
HTTP trazem status/código/campos rejeitados sem expor tokens ou Base64. Não há
retry cego de alterações de perfil. A confirmação da API antecede a persistência.

## Permissões, parâmetros e recuperação

Todos os parâmetros condicionais são opcionais no registro. A ação selecionada
valida quais podem ser usados; parâmetros de outra ação são rejeitados, com a
exceção de `id` em Links, que é ignorado fora de Remove. Não deixe
`imagem` preenchida ao executar Revert, por exemplo.

Rules, Links e anúncios exigem Gerenciar Mensagens; Profile exige Gerenciar
Servidor; Role exige Gerenciar Cargos; Role-all exige também Gerenciar Servidor. Cargo moderador configurado e hierarquias
continuam valendo. As permissões de canal são verificadas para o bot, permitindo
uso por um administrador autorizado que não possa escrever diretamente no destino.

Em Links, Publish confirmado cria uma substituta quando o Discord confirma
`404 / 10008 — Unknown Message`, arquiva a referência antiga e salva o novo ID.
Se a mensagem existe, mantém o ID e atualiza. Erros de permissão, canal ausente
ou transporte não criam outra publicação. Rules e Announcement preservam o
fluxo de Remove confirmado para reconciliar uma mensagem ausente. Links Remove
continua gerenciando somente entradas do catálogo. Se `pending` indicar resultado HTTP incerto, confira o canal
e o estado antes de qualquer repetição; mantenha backups e não apague o JSON.

## Cargos individuais e em massa

```text
/role ação:add usuário:@Pessoa cargo:@Participante
/role ação:remove usuário:@Pessoa cargo:@Participante
/role-all ação:add cargo:@Participante
/role-all ação:remove cargo:@Participante
```

Usuário e cargo são os seletores nativos USER/ROLE. O comando individual confirma
por REST que o usuário pertence à guilda e funciona sem a intent GUILD_MEMBERS.
Não existe parâmetro de usuário STRING nem valor especial `all`.

Role-all é opt-in em `[commands] role-all = true` e requer a intent de membros no
TOML global e no Portal. Lista todos os membros com paginação REST, conta os humanos
elegíveis e mostra confirmação. Ignora bots, estado já desejado, dono/autor/bot,
membros em triagem e alvos fora da hierarquia. Cargos gerenciados/especiais,
protegidos e administrativos em massa são bloqueados.

Só os usuários do grupo confirmado são processados, de forma sequencial e
assíncrona, usando rate limits do Serenity. Permissões, cargo e membro são
reconsultados antes de cada alteração. Perda de autorização/erros gerais param
a execução; falhas individuais são contadas. O resumo distingue sucessos,
falhas, ignorados e não processados. Não há reversão automática de sucessos.

Uma operação/confirmação de cargos bloqueia outra da mesma guilda, inclusive o
comando individual. Outras guildas continuam independentes. Progresso e resumo
ficam em `state.json.role_operations` e nos logs, sem tokens ou dados de imagens.
A resposta é atualizada enquanto o token de interação é válido. Se a operação
ultrapassar esse prazo, confira o resumo persistido/logs; a expiração da resposta
não interrompe o trabalho confirmado. Veja [configuração e recuperação](roles.md).
