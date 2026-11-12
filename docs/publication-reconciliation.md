# Publicações V2 — correção 50080 e reconciliação

## Causas investigadas

O construtor enviava `sticker_ids: []` em todo PATCH. Esse campo não é editável
e o Discord rejeitou a requisição com HTTP 403/código **50080**. A consulta real
de metadados da publicação afetada confirmou mensagem tipo 0, flags 0, nenhum
sticker/anexo, um embed e um componente: a causa era o campo enviado, não a
presença de stickers na mensagem original. O código 50080 significa operação
de sticker não suportada, não falta genérica de permissão.
[Códigos oficiais](https://docs.discord.com/developers/topics/opcodes-and-status-codes#json-json-error-codes).

O PATCH corrigido omite `sticker_ids`. Na conversão sem stickers, mantém o
Message ID, ativa `IS_COMPONENTS_V2`, limpa `content`/`poll` e `embeds` e envia
os componentes/anexos preservados. Em mensagens já V2 não reenvia os campos
legados. O GET original acontece antes da escrita. A flag não pode ser removida.
[Components V2](https://docs.discord.com/developers/components/using-message-components),
[parâmetros de edição](https://docs.discord.com/developers/resources/message#edit-message).

Se a mensagem realmente contém stickers, não tentamos alterá-los. Publish exibe
uma confirmação específica de **Substituir publicação**. A nova mensagem recebe
seu ID somente após sucesso confirmado; o ID antigo é arquivado e a mensagem
original permanece no canal. Não há DELETE automático. Falha/Cancelamento
preservam a original. Um resultado incerto bloqueia outra tentativa até a
reconciliação; não gera outra mensagem automaticamente.

O bloqueio de Announcements investigado vinha de um **pending de Links** da
mesma guilda. O journal antigo guardava somente `chave:operação` e mantinha o
bloqueio mesmo após uma rejeição definitiva. Não registrava ação, status HTTP,
payload esperado nem recibo. Portanto, não é seguro deduzir o resultado desse
registro legado apenas porque a mensagem ainda está em V1.

## Estados corrigidos

- **Sucesso:** grava recibo e finaliza publicação, documentos/imagens e histórico.
- **Rejeição definitiva:** guarda status HTTP/código Discord e arquiva o pending;
  preserva publicações e libera novos comandos. Não repete a chamada rejeitada.
- **Resultado desconhecido:** timeout, resposta malformada, 5xx ou falha de disco
  depois do envio mantêm journal e bloqueio. HTTP 408/409/425/429 também são
  tratados conservadoramente, sem liberação automática.
- **Reconciliação positiva:** confere canal/guilda, autoria, Message ID e conteúdo
  esperado; para Create sem recibo, exige nonce da operação e conteúdo. Não basta
  encontrar uma mensagem parecida. Delete só conclui pela ausência de um alvo
  completamente identificado, confirmada por `404/10008`.
- **Inconclusivo/legado:** preserva tudo e informa módulo, ação quando disponível,
  operação, canal e mensagem. Uma busca recente sem resultados não prova falha.

O JSON mantém `pending`, adiciona `pending_operation` opcional e
`publication_history`. O payload/rascunho temporário sai do journal ao finalizar;
o histórico guarda evidência e identificadores, sem Base64 ou binários. O TOML
continua sendo fonte do conteúdo de Links. Arquivos antigos continuam legíveis.

## Ferramenta administrativa

A ferramenta apenas consulta o Discord e, com `--apply`, reconcilia o JSON local.
Não publica, edita nem exclui mensagens. Ela exige o lock exclusivo de dados;
**pare o bot normalmente antes de executá-la**. Use o mesmo CONFIG_DIR/DATA_DIR
da instância e mantenha DISCORD_TOKEN somente no ambiente ou `.env` local.
Nunca informe a credencial como argumento.

No Windows, a partir da raiz do repositório:

```powershell
$env:CARGO_TARGET_DIR='target/reconciliation'
cargo run --bin reconcile-publication -- --guild GUILD_ID
```

Sem `--apply`, não modifica `state.json`. Se encontrar evidência conclusiva,
revise o diagnóstico e repita com `--apply`. A gravação preserva backup exato
`state.pre-reconcile-*.json.bak`, histórico e dados dos demais módulos/guildas.
Não exclua esses backups. Após salvar, reinicie o bot e abra uma nova prévia.

### Mensagem confirmada fora da busca recente

Para journals novos, pode indicar o ID conhecido da mensagem. A ferramenta ainda
exige nonce/recibo/alvo e conteúdo correspondentes à operação registrada:

```powershell
cargo run --bin reconcile-publication -- --guild GUILD_ID --message-id MESSAGE_ID
# Depois de revisar a evidência positiva:
cargo run --bin reconcile-publication -- --guild GUILD_ID --message-id MESSAGE_ID --apply
```

Não há adoção automática de mensagem em journal legado sem identidade/rascunho.
Se não for possível comprovar sucesso ou rejeição, mantenha o bloqueio, preserve
o backup e investigue os logs/requisição original. Não tente reenviar cegamente.

### Pending legado com rejeição comprovada nos logs

Para o caso de Links/50080, confira no diagnóstico o identificador completo e
confirme que os logs correspondem àquela tentativa. O comando abaixo **atesta
uma evidência externa pelo operador**; a existência de uma mensagem V1, sozinha,
não prova a rejeição. Não use essa opção para timeouts ou resultados desconhecidos.

```powershell
cargo run --bin reconcile-publication -- --guild GUILD_ID --reject-http 403 --reject-code 50080 --confirm-pending "links:OPERATION_ID"
# Após revisar os logs, a autoria/canal e o pending exato:
cargo run --bin reconcile-publication -- --guild GUILD_ID --reject-http 403 --reject-code 50080 --confirm-pending "links:OPERATION_ID" --apply
```

A ferramenta confere a guilda real do canal, autoria e referência. Recusa
confirmação divergente, evidência não reconhecida, recibo de sucesso contraditório
ou conversão já observada incompatível com a alegação legada. Registra
`operator_attested_rejection`, com código/status e backup, sem perder Message ID.
Não edite `pending` manualmente nem copie estado de outra guilda.

## Homologação

1. Pare a instância e use a ferramenta primeiro sem `--apply`. Resolva o legado
   somente com evidência suficiente. Reinicie com o binário corrigido.
2. Publique Links sobre a mensagem V1 sem stickers e compare o mesmo Message ID.
   Repita já em V2; confira cor, conteúdo e ambos os botões existentes.
3. Em um quadro de teste com stickers, cancele a substituição e confira a original.
   Depois confirme e verifique nova publicação, ID arquivado e original preservada.
4. Crie/edite um anúncio e confira imagem, thumbnail/footer e ID de edição.
5. Confira rejeições conhecidas sem bloqueio permanente, pending incerto sem
   reenvio e recuperação após reiniciar. Não provoque perdas de conexão em produção.
6. Teste duas guildas e registre a evidência separadamente, sem tokens/convites
   privados. A leitura real de metadados desta investigação não homologa a
   publicação corrigida ou as interações no Discord.

Os testes automatizados usam HTTP local e diretórios temporários. Cobrem payload
sem sticker_ids, mensagens V1/V2, substituição, rejeição, timeout após envio,
reinício, recibo/nonce, busca vazia, confirmação manual, exclusão conhecida,
canal de outra guilda e sanitização. Nenhuma credencial real é usada nos testes.

## Arquivos desta correção

- `src/services/components_v2.rs`: payload, metadados, journal e substituição segura.
- `src/services/publication_state.rs`: classificação, recibos, histórico e reconciliação.
- `src/services/publishing.rs`: journal e nonce de anúncios, preservando os outros fluxos.
- `src/services/storage.rs` e `src/services/mod.rs`: campos opcionais compatíveis e backups.
- `src/modules/links.rs` e `src/modules/content_commands.rs`: diagnóstico antes da prévia
  e confirmação específica de substituição, sem novas raízes ou ações slash.
- `src/core/diagnostics.rs`: códigos/status separados e resposta sanitizada.
- `src/bin/reconcile-publication.rs`: ferramenta administrativa com lock exclusivo.
- `tests/command_refactor.rs` e `tests/components_v2.rs`: regressões sem Discord real.
- `README.md`, `docs/architecture.md`, `docs/links.md`, `docs/validation-v2.md`
  e este guia: comportamento, evidências e procedimento de homologação.

Nenhuma configuração TOML, arquivo operacional, frontend da Activity ou módulo
Rules/Profile/Role foi alterado. O registro legado real permanece pendente até a
revisão administrativa com a instância parada; não foi liberado à força.
