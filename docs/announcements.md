# Announcements — imagens, prévia e edição

## Upload na versão instalada

O projeto usa Poise 0.6.2 com Serenity 0.12.5. O Discord oferece `FILE_UPLOAD`
em modals, mas essa versão do Serenity não contém `CreateFileUpload`, `CreateLabel`
nem o modelo correspondente para interpretar a submissão. Os builders aparecem
na documentação `next`, não no pacote instalado. Para evitar uma migração ampla
de bibliotecas, esta versão utiliza o fallback **Attachment no slash command**.
O formulário continua nativo, para título, mensagem, cor e URL alternativa.
O upload não é substituído por uma URL obrigatória.

Referências verificadas:

- [Builders do Serenity 0.12.5](https://docs.rs/serenity/0.12.5/serenity/builder/index.html).
- [Builders da próxima versão](https://serenity-rs.github.io/serenity/next/serenity/builder/index.html).
- [Componentes oficiais do Discord](https://github.com/discord/discord-api-docs/blob/main/developers/components/reference.mdx).
- [Upload e edição de mensagens](https://github.com/discord/discord-api-docs/blob/main/developers/resources/message.mdx).
- [EditAttachments do Serenity instalado](https://docs.rs/serenity/0.12.5/serenity/builder/struct.EditAttachments.html).

## Criar e visualizar

```text
/announcement imagem:<arquivo>
/announcement mensagem:"Aviso importante" imagem:<arquivo>
/announcement mensagem:"Aviso importante" url_imagem:https://example.org/banner.png
/announcement
```

Selecione o arquivo no campo nativo `imagem` **antes de enviar o comando**.
Sem `mensagem`, clique no botão privado Preencher formulário. O modal oferece:

| Campo | Uso |
|---|---|
| Título | Opcional; vazio utiliza o cabeçalho oficial da comunidade |
| Mensagem | Obrigatória, multilinha |
| Cor hexadecimal | Inicializada com a cor do template TOML |
| URL alternativa | Opcional; não preencher quando enviar upload |

Também é possível informar a URL alternativa no parâmetro `url_imagem`. Não
informe URLs nos dois lugares. Upload e URL simultâneos geram erro de validação;
nenhuma fonte é descartada silenciosamente. URLs externas HTTP/HTTPS são usadas
diretamente pelo Discord, sem download pelo bot. Sem nova fonte, a imagem do
template, se configurada, é preservada; use `imagem_acao:remover` para descartá-la.
Anúncios somente textuais continuam funcionando.

O bot mostra uma **prévia privada** com Publicar/Cancelar. Uploads são anexados à
resposta efêmera, referenciados por `attachment://nome-do-arquivo`, e os bytes
permanecem apenas no rascunho em memória. Confirmar envia os mesmos bytes junto
com o embed para o canal configurado. Não exige outro upload. Cancelar ou deixar
expirar a confirmação em 120 segundos descarta o rascunho sem publicação pública.
O formulário tem prazo de 180 segundos após a abertura. Não há hospedagem própria.

Sem título personalizado, o construtor utiliza
`📢 Comunicado oficial da administração da comunidade — {guild.name}`, obtendo
o nome atual pela API do Discord. O título genérico legado `📢 Anúncio` e o
fallback interno `announcements` também usam esse padrão, sem reescrever TOMLs.
Outros títulos do template ou do administrador são preservados. Preview, Publish
e `/announce` compartilham essa regra. A largura do embed continua sendo definida
automaticamente pelo cliente Discord.

## Validações e limites

São aceitos **PNG, JPEG, WebP e GIF**, com extensão coerente, MIME compatível
quando informado e conteúdo real decodificável. O limite da aplicação é **8 MiB**
por arquivo, reduzido ao limite indicado pela interação quando este for menor.
Profile e Announcements utilizam o mesmo downloader, incluindo suporte a anexos
temporários `/ephemeral-attachments/`. O arquivo é baixado antes de abrir o modal,
preservando a assinatura da URL; veja [diagnóstico e homologação](attachments.md).
O download lê os bytes com limite progressivo, timeout e sem redirecionamentos;
aceita somente URLs de anexos oficiais recebidos do Discord. Não executa arquivos.

A decodificação tem orçamento de 128 MiB, sem um limite fixo de resolução.
Uma imagem válida de 1254 × 1254 é aceita. GIFs são validados quadro a quadro,
com orçamento acumulado de 128 MiB e até 1000 frames. Arquivos incompatíveis com
esses limites geram erro claro. Os bytes originais são enviados, mantendo o
formato e a animação; não há conversão ou redimensionamento silencioso.

Falhas distinguem validação, processamento, download e HTTP. Logs não contêm
bytes, Base64, tokens ou URLs privadas. O bot precisa das permissões usuais de
envio/embed e de **Anexar arquivos** quando houver um novo upload.

## Editar, substituir e remover imagem

`/announcement ação:edit` mantém a seleção/autocomplete dos anúncios registrados.
Use `imagem_acao` para escolher explicitamente:

- `manter`: preserva a imagem atual; não aceita nova fonte simultânea.
- `substituir`: exige um novo `imagem` ou uma URL alternativa.
- `remover`: remove a imagem principal; não aceita nova fonte simultânea.

Sem `imagem_acao` e sem nova fonte, o padrão é **manter**, inclusive quando o
campo URL do formulário fica vazio. Uma nova fonte, sem ação explícita, substitui
a anterior. Exemplos:

```text
/announcement ação:edit mensagem:"Texto corrigido"
/announcement ação:edit imagem:<outro arquivo>
/announcement ação:edit url_imagem:https://example.org/novo-banner.png
/announcement ação:edit imagem_acao:remover
```

O bot recupera a mensagem original e valida autoria/canal. Em edições, envia
explicitamente `attachments` com os IDs a preservar. Alterar apenas o texto
mantém os anexos atuais. Substituir/remover descarta somente o anexo identificado
como imagem principal; outros arquivos da mensagem são preservados. A publicação
mantém Guild ID, Channel ID, Message ID e identificador interno.

Se o anexo principal sumiu, Manter falha sem apagá-lo silenciosamente. Escolha
Substituir ou Remover para resolver esse caso. Uma mensagem inteira apagada segue
o fluxo existente de remoção/reconciliação dos anúncios gerenciados.

## Persistência e reinicialização

`data/guilds/<guild_id>/state.json` conserva as referências da publicação e
`announcement_images`, um mapa de metadados por identificador de anúncio:

- Anexo: tipo, **ID do arquivo na mensagem pública** e nome do arquivo.
- URL externa: tipo e URL configurada.
- Sem imagem: nenhuma entrada no mapa.

O ID do upload original do administrador não é reutilizado como ID público.
O documento do anúncio referencia anexos por `attachment://nome`, e não pela
URL temporária da interação. Após reiniciar, o bot busca a mensagem original
para recuperar seus anexos e uma URL atual para a prévia. URLs assinadas legadas
que correspondem a anexos da própria publicação são reconhecidas pelo caminho.
Não há Base64 no JSON nem arquivos persistentes de rascunho. Estados antigos
sem o novo mapa continuam sendo aceitos. Os TOMLs existentes não são reescritos.

## Identidade automática e envio rápido

Criação, prévia, edição e `/announce` usam o mesmo construtor. A API fornece o
nome e o ícone atuais da guilda. Sem ícone, não há thumbnail; o avatar do bot não
é usado como substituto. O footer é sempre:

```text
Comunicado oficial • {nome do servidor} • Ograda Solutions Bot
```

Thumbnail e footer antigos do template são ignorados somente para anúncios.
A imagem principal permanece independente do ícone. Título, conteúdo, cor e
imagem principal opcional são preservados.

```text
/announce mensagem:"Aviso rápido"
/announce mensagem:"Novo banner" imagem:<arquivo>
/announce mensagem:"Aviso" canal:#geral imagem:<arquivo>
/announce mensagem:"Aviso" url_imagem:https://example.org/banner.png
```

`/announce` continua sendo imediato e não cria anúncio gerenciado. A opção de
canal permanece limitada à guilda atual; não exige escrita direta do administrador
no canal, mas mantém sua autorização para usar o bot. Menções são desabilitadas.

## Homologação no Discord

1. Reinicie com o binário atualizado e confira `imagem` como Attachment nos dois
   comandos, e `imagem_acao` em Announcement. Não haverá upload dentro do modal
   nesta versão; selecione o arquivo no slash command.
2. Crie um anúncio sem imagem e outro com PNG 1254 × 1254. Confira prévia privada,
   thumbnail da guilda, footer, Publicar e Cancelar; cancelar não deve publicar.
3. Publique PNG, JPEG, WebP e GIF; confira imagem no embed e animação do GIF.
4. Teste MIME/extensão incorretos, arquivo malformado, acima de 8 MiB e duas
   fontes simultâneas. Os erros não devem criar publicações.
5. Edite somente título/mensagem e deixe URL vazia. Confira preservação da imagem,
   dos arquivos necessários e do Message ID.
6. Teste upload → upload, upload → URL, URL → upload e remoção explícita. Confira
   que somente a imagem principal muda e que a prévia corresponde ao resultado.
7. Reinicie o bot e edite o mesmo anúncio. Confira os IDs públicos e os metadados
   no JSON, sem bytes de imagem ou URLs temporárias da interação.
8. Teste `/announce` sem imagem, com upload e canal opcional. Teste bot sem
   Anexar arquivos e usuário sem autorização; não deve ocorrer envio indevido.
9. Repita em outra guilda com nome/ícone próprios e em uma guilda sem ícone.
   Confira que conteúdo, arquivos, IDs e identidade não atravessam guildas.

Os testes automatizados usam imagens geradas, diretórios temporários e HTTP local.
Não usam credenciais reais nem publicam no Discord. O teste real de Gateway,
formulários e apresentação visual continua sendo uma etapa de homologação.

## Validação técnica — 12/11/2026

- `cargo fmt --all -- --check`: aprovado.
- `cargo clippy --all-targets -- -D warnings`: aprovado.
- `cargo test`: 114 testes aprovados, incluindo multipart, edição e reinicialização.
- `cargo build`: aprovado; binário Windows em `target/stabilization/debug`.
- `docker compose config --quiet`: aprovado.
- `docker compose build`: aprovado, imagem `discordserverbot-bot:latest` construída.

A instância do bot em execução não foi reiniciada. Configurações privadas e
dados reais foram preservados. Após encerrar a instância anterior, inicie o
executável atualizado para registrar os novos parâmetros slash e homologar.
