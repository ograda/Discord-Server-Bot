# Links — administração e convites

`/links` exige `ação`, uma escolha nativa STRING: `preview`, `publish`, `update`,
`reset`, `add` ou `remove`. O único parâmetro opcional é `id`, com autocomplete
de links existentes. Ele é utilizado apenas em Remove e ignorado nas demais ações.
Não existe ação padrão nem parâmetro de quadro no slash command.

## Catálogo e formulário

Preview, Publish e Add utilizam o único quadro ou o quadro identificado por
`default`. Se houver vários sem esse padrão, apresentam uma seleção privada.
Board ID identifica um quadro; Link ID é único dentro dele. A seleção e o
autocomplete distinguem links com o mesmo ID em quadros diferentes.

`/links ação:add` oferece o botão privado **Preencher formulário**, que abre o
modal nativo **Adicionar link**. Essa etapa permite concluir a autorização REST
e a escolha do quadro antes de abrir o modal, sem exceder o prazo inicial da
interação. O modal contém exatamente cinco campos, nesta ordem:

| Campo | Obrigatório |
|---|---|
| ID | Sim |
| Título | Sim |
| Descrição | Não |
| URL | Sim, HTTP/HTTPS sem credenciais |
| Emoji | Não |

Após validar os campos e a unicidade do ID no quadro, o bot mostra uma prévia
privada com **Salvar/Cancelar**. Somente Salvar modifica o catálogo. O bot grava
`config/guilds/<nome>/links.toml`, atualiza a memória e não publica. Quando o
catálogo é herdado, cria o arquivo específico com o conteúdo efetivo. Nunca
reescreve os defaults compartilhados. A escrita usa validação, backup do arquivo
existente, verificação de concorrência e substituição atômica; `toml_edit` conserva
comentários quando possível. Arrays inline podem ser normalizados, com o original
preservado no backup.

`/links ação:remove id:whatsapp` localiza uma entrada. Sem ID, apresenta seleção
privada dos links. Se o ID existir em vários quadros, pede a seleção inequívoca.
Após confirmação, remove somente a entrada do TOML específico e da memória;
preserva outros links, quadros, publicações e Message IDs. Uma publicação antiga
só muda após Publish. Remover o último link mantém o quadro vazio, sem ressuscitar
entradas herdadas após reiniciar.

## Apresentação

Cada quadro utiliza um único Container Components V2, com cor lateral do TOML
(padrão #5865F2), título destacado, introdução e links Markdown. Emoji, título
e legenda clicável ficam na mesma linha; a descrição fica na seguinte. Blocos
consecutivos têm duas quebras e preservam quebras internas. `label` mantém o
fallback **Abrir link**; o convite usa **Link do convite** e fica por último.

O ícone da guilda aparece como Thumbnail em uma Section, quando disponível.
Não há outro embed nem avatar substituto. Uma Gallery conserva a imagem
opcional, e rodapé/texto do quadro são Text Displays. Separator e Action Row
dos botões ficam dentro do Container. O construtor valida 4.000 unidades UTF-16
de texto total, RGB e até 40 componentes contando filhos, sem truncamento.
Controles e avisos privados também entram na validação. O Discord decide a
largura final. Rules e Announcements continuam utilizando seus embeds.

## Convite especial do Discord

`kind = "discord_invite"` identifica explicitamente o convite principal. URLs
oficiais `discord.gg/<código>` e `discord.com/invite/<código>` também são reconhecidas
para compatibilidade. Outras páginas do Discord não são classificadas como convites.
Um quadro aceita somente um convite principal; múltiplos convites geram erro de
configuração. Ele aparece sempre por último no quadro, sem reordenar o TOML.

A linha do convite mostra **Link do convite** clicável e a mensagem recebe dois botões:

- **📋 Copiar convite** responde privadamente com a URL configurada em bloco de
  código, para copiar manualmente. Não acessa nem afirma alterar a área de transferência.
- **🎮 Convidar amigos** usa o custom_id estável `links-invite-friends`. Com
  `[activities.invite] enabled=false` (padrão), responde privadamente:
  “Lembra o besta do Ograda que essa funcionalidade ainda não está disponível”.
  Quando habilitada em uma guilda elegível, utiliza LAUNCH_ACTIVITY (tipo 12)
  para iniciar a Activity da mesma aplicação. Não abre URLs externas. O diálogo
  nativo é aberto por um botão dentro da Activity. Veja [Activity](activity-invite.md).

Membros comuns podem usar esses botões. O handler de cópia valida guilda,
canal, Message ID e autoria da publicação gerenciada; não exige autorização
administrativa. O JSON conserva a URL e o identificador do botão da publicação
confirmada, permitindo o uso após reinicialização. Update, Add e Remove não
trocam o convite de uma mensagem já publicada; Publish atualiza esse estado.
O bot não gera convites nem introduz redirecionamentos.

## Exemplo TOML

Os códigos abaixo são ilustrativos; substitua-os por endereços autorizados.

```toml
[[boards]]
id = "default"
title = "🔗 Links da Comunidade"
description = "Aqui você encontra os principais links e grupos da nossa comunidade."
channel = "links"
color = 10181046

[[boards.links]]
id = "whatsapp"
title = "Grupo do WhatsApp"
description = "Participe também do nosso grupo de WhatsApp."
url = "https://chat.whatsapp.com/EXEMPLO"
emoji = "📱"
label = "Entrar no grupo"

[[boards.links]]
id = "discord"
kind = "discord_invite"
title = "Convite do Discord"
description = "Compartilhe o convite da nossa comunidade com seus amigos."
url = "https://discord.gg/EXEMPLO"
emoji = "🎮"
```

O formato legado `title` + `[[entries]]` continua aceito como quadro `default`.
O JSON continua responsável pelas referências operacionais; conteúdo antigo nele
não substitui o catálogo TOML.

## Preview, Publish, Update e Reset

`/links ação:preview` mostra a memória atual em privado, incluindo os botões do
convite. Esses controles de teste expiram com a sessão privada. Não publica.

`/links ação:publish` usa o mesmo construtor e oferece **Editar/Publicar/Cancelar**.
Editar abre o formulário de apresentação do quadro (título, introdução, cor,
imagem e rodapé). As edições permanecem na prévia até confirmar Publicar; nesse
momento são salvas no TOML específico e o envio é realizado. Se o HTTP falhar
depois da gravação, o TOML permanece salvo e o erro informa o problema remoto.
Publicar cria a mensagem uma única vez ou edita a original, preservando Message ID.
Se a mensagem original foi apagada e o Discord confirma `404 / 10008 — Unknown
Message`, a confirmação de Publish cria uma substituta. A referência anterior
fica em `state.archived_publications`, e a nova publicação recebe um novo ID.
Outros erros não disparam recriação. O catálogo TOML permanece intacto.
Cancelar/expirar não grava nem publica. Revisões obsoletas e cliques duplicados
são bloqueados.

`/links ação:update` valida e recarrega o arquivo efetivo para memória, informando
a origem, sem mudar publicações. Arquivos inválidos preservam o snapshot anterior.
`/links ação:reset` confirma a restauração dos defaults para a guilda, cria backup
do arquivo específico e recarrega, sem publicar nem apagar referências.

## Homologação no Discord

1. Reinicie com o binário atualizado e reabra `/links`. Confira somente `ação`
   obrigatória e `id` opcional; ações desabilitadas devem desaparecer por guilda.
2. Teste Preview com um quadro, com `default` e com vários sem padrão. Confira
   Container único, cor, ícone, espaçamento, Markdown e convite por último.
3. Teste Add, os cinco campos, URL inválida, duplicidade, Cancelar e Salvar.
   Confira o TOML específico, backup e memória; a mensagem pública deve permanecer igual.
4. Teste Remove com ID, sem ID e com IDs repetidos em quadros distintos.
   Cancele e confirme. Confira preservação das demais entradas e das referências.
5. Teste Publish, Editar e Cancelar. Publique duas vezes e confira o mesmo Message ID.
6. Como membro comum, use Copiar convite e confira a resposta privada; Convidar
   amigos deve mostrar somente o aviso privado. Confira o Link do convite no
   quadro. Reinicie e teste novamente os botões públicos.
7. Modifique o convite no TOML e use Update: a publicação deve manter seu convite
   anterior até Publish. Teste erro por múltiplos convites e por limite de descrição.
8. Teste Reset e duas guildas, verificando backups e isolamento de conteúdo/estado.

Os testes automatizados usam apenas diretórios temporários e HTTP local. A
homologação de Gateway, modals e aparência real é realizada separadamente.

## Migração de publicações V1

Preview não modifica mensagens públicas. Publish informa a conversão irreversível
na confirmação e utiliza PATCH na mesma mensagem, mantendo o Message ID. O payload
ativa IS_COMPONENTS_V2, limpa content/poll com null e embeds/sticker_ids com arrays
vazios; conserva os anexos existentes. Isso é permitido pelo endpoint oficial de
[edição de mensagem](https://docs.discord.com/developers/resources/message#edit-message).
Não há DELETE/POST ou duplicação durante essa conversão. Mensagens já V2 também
são atualizadas por PATCH. Campos JSON de outros módulos permanecem intactos.

Falha deixa referências/pending preservados e não cria outra mensagem. A flag não
pode ser removida dessa mensagem; voltar ao executável V1 não desfaz a conversão.
O caso distinto de mensagem já apagada continua usando recuperação por 404/10008
após confirmação, arquivando o ID anterior. Veja [implantação](deployment-v2.md)
e o roteiro completo de [homologação V2](validation-v2.md).
