# Anexos de interação — Profile e Announcements

## Causa e correção

A implementação anterior tinha dois downloads separados: o de Profile e o de
Announcements. Ambos restringiam o caminho a `/attachments/`. A rejeição de uma
URL oficial com `/ephemeral-attachments/` foi reproduzida antes da alteração;
ocorria na validação local, sem chegar à API/CDN. A mensagem “somente anexos
oficiais do Discord são aceitos” vinha de `announcements/media`; Profile utilizava
uma mensagem própria para a mesma restrição.

Os dois módulos agora usam `src/services/attachments.rs`. O serviço recebe o
objeto `Attachment` resolvido pelo Serenity e valida com `url::Url`:

- HTTPS obrigatório, hostname exato `cdn.discordapp.com` ou `media.discordapp.net`.
- Rotas `/attachments/<id>/<id>/<arquivo>` e
  `/ephemeral-attachments/<id>/<id>/<arquivo>`.
- IDs numéricos válidos, arquivo presente, ausência de credenciais, fragmentos,
  portas alternativas ou separadores de caminho disfarçados.
- Nenhuma aceitação por substring, sufixo de domínio ou URL externa.

Os parâmetros assinados `ex`, `is` e `hm` são mantidos na requisição. O downloader
não envia o token do bot para a CDN, não segue redirecionamentos e mantém timeout
de conexão de 10 segundos e total de 30 segundos. A leitura é limitada enquanto
recebe os bytes, respeitando 4 MiB em Profile e até 8 MiB em Announcements.
O tamanho real deve corresponder ao declarado pelo Attachment.

Profile continua validando o conteúdo e produzindo Data URI Base64 para o avatar.
Announcements continua validando o conteúdo e publicando os bytes como anexo
do próprio bot, com referência `attachment://filename` no embed.

## Resolução pelo Serenity e expiração

No Serenity 0.12.5, opções slash do tipo Attachment são resolvidas por
`CommandDataResolved.attachments`, sem reescrever `Attachment.url`. O teste de
regressão confirma a preservação de uma URL efêmera assinada.

O modelo `ModalInteractionData` dessa versão contém `custom_id` e componentes,
mas não oferece o suporte a File Upload/resolved attachments dos modals novos.
O projeto mantém o fallback Attachment no slash command, documentado no
[guia de Announcements](announcements.md), sem atualizar bibliotecas.

Em Announcement, o arquivo é baixado e validado logo após a autorização, antes
da seleção de anúncio ou abertura do formulário. Depois disso, a criação da
prévia/publicação usa os bytes em memória, sem depender da assinatura original.
Profile baixa imediatamente no fluxo Avatar, sem passar por formulário.

Respostas HTTP 403/404 indicam anexo indisponível ou possivelmente expirado;
o administrador deve enviar o arquivo novamente. O bot não remove a assinatura,
não tenta inventar outra URL nem repete a requisição automaticamente.

## Diagnósticos sem dados sensíveis

Falhas registram módulo, tipo de entrada, tipo do anexo (`conventional` ou
`ephemeral`), etapa, hostname, caminho sem query e status HTTP quando disponível.
Exemplos de etapas: `url_validation`, `size_validation`, `request`, `read_response`.
Não são registrados query strings, assinaturas, credenciais, corpo da resposta
de erro ou bytes da imagem. O Debug do resultado do download mostra apenas tamanho.

## Homologação no Discord

1. Encerre a instância antiga e abra
   `target/stabilization/debug/discord-server-bot.exe` a partir da raiz do projeto.
   Se usar Docker, reconstrua a imagem antes de iniciar o contêiner atualizado.
2. Envie um **novo** PNG 512 × 512 em `/profile ação:avatar imagem:<arquivo>`.
   Confirme a alteração do avatar na guilda atual. Repita com PNG 1254 × 1254,
   sem normalização, e confirme que as dimensões não são alteradas silenciosamente.
3. Execute `/announcement imagem:<novo PNG>`, preencha o formulário e confirme
   que a imagem aparece na prévia privada. Publique e confira a imagem principal,
   thumbnail da guilda e rodapé automático na mensagem pública.
4. Edite apenas o texto do anúncio e confira que imagem e Message ID permanecem.
   Cancele outra criação para verificar que não há publicação pública.
5. Repita em outra guilda. Se ocorrer erro, confira hostname/caminho/tipo/etapa
   no log e o status HTTP. Não compartilhe a URL completa com sua assinatura.

O caminho é fornecido pelo Discord, não escolhido no comando pelo usuário.
Para confirmar uma ocorrência efêmera real, confira `attachment_kind="ephemeral"`
no diagnóstico de falha, quando houver. Os testes automatizados cobrem as duas
rotas nos dois hostnames, assinaturas, hostnames falsificados, redirecionamentos,
expiração/indisponibilidade, resolução pelo Serenity e PNG usado pelos dois módulos.

Validação local: 123 testes aprovados, formatação, Clippy com warnings negados e
build Windows aprovados. Nenhuma chamada real ao Discord foi usada nesses testes.

Referência: [URLs assinadas da CDN na documentação oficial do Discord](https://github.com/discord/discord-api-docs/blob/main/developers/reference.mdx).
