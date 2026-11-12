# Profile — diagnóstico de interações

## Requisição envolvida

O erro informado acontece imediatamente ao executar /profile, antes de clicar
em Restaurar perfil. Nesse ponto, a primeira requisição do handler é o defer:

    POST /interactions/{id}/{token}/callback
    type: 5 (defer privado)

Esse callback é independente de PATCH /guilds/{guild}/members/@me. O campo
action="revert" é também o padrão inferido para /profile sem ação; não prova
que o endpoint de alteração do perfil tenha sido chamado.

O defer já existia antes da autorização, leitura do TOML e processamento de imagens.
O log antigo não continha rota nem idade da interação, portanto não permite
demonstrar retroativamente a duração exata do atraso. O tratamento de erro tinha
um problema adicional confirmado: quando o defer falhava, o estado do Poise ainda
indicava ausência de resposta e o handler tentava outra resposta inicial.

## Fluxo corrigido

1. Registrar e reservar o ID da interação antes de qualquer resposta inicial.
2. Enviar o defer imediatamente, sem autorização REST ou processamento antes dele.
3. Marcar a interação como reconhecida no Poise somente após sucesso.
4. Executar autorização e preparar o perfil.
5. Apresentar confirmação privada usando o token do comando já deferido.
6. Reconhecer o clique do botão por seu próprio ID/token antes de alterar o perfil.
7. Desativar os controles e executar a operação somente após confirmação válida.
8. Enviar o resultado por follow-up privado; nunca por outra resposta inicial.

Cada ID tem no máximo uma tentativa de callback inicial nesta instância, inclusive
quando a tentativa falha ou seu resultado é incerto. A proteção também cobre
consumidores concorrentes. Cancelamento e confirmação expirada não autorizam a
alteração. Modals têm callbacks separados para abertura e submissão, com
reconhecimento anterior à interpretação/processamento dos dados.

Se o defer original falhar, não há tentativa de responder novamente por aquele
callback. Se o botão falhar mas o comando pai continua válido, o diagnóstico pode
ser enviado pelo follow-up do comando pai. Não há fallback público nem mensagens
diretas automáticas.

## Logs e códigos

| Etapa | Requisição |
|---|---|
| SlashDefer | POST callback do slash command |
| ConfirmationAck | POST callback do botão |
| ModalOpen / ModalSubmitAck | POST callbacks distintos do formulário |
| ConfirmationDisable | PATCH da mensagem privada de confirmação |
| Progress / Result / ErrorNotice | POST follow-up privado |
| profile_http | GET/PATCH de membro da guilda, com rota explícita |

As rotas são templates: tokens e URLs reais não entram nos logs. O diagnóstico
registra PID, ID da interação, guilda, comando, ação, etapa, status HTTP e código
Discord. Antes do callback, registra interaction_age_ms e
initial_deadline_passed; após sucesso, registra a duração da requisição.
A idade é estimada pelo Snowflake e relógio local: diferença de relógios também
deve ser considerada ao interpretar o prazo.

- **10062:** interação desconhecida/expirada; não é diagnóstico de permissão.
- **10015:** webhook/token da resposta indisponível ou expirado.
- **40060:** interação já reconhecida; investigar resposta duplicada ou outra instância.
- **50001:** falta de acesso ao recurso.
- **50013:** permissão insuficiente.

Se falhar somente o follow-up final, o diagnóstico informa que o perfil já foi
processado e orienta conferir o resultado antes de repetir. Uma falha ao entregar
o próprio diagnóstico não é apresentada como falha da atualização do avatar.

## Instâncias e homologação

Na consulta local foi encontrada uma instância Windows do bot e nenhum contêiner
do bot em execução. Isso não exclui um deployment em outra máquina. O lock
existente impede duas instâncias usando o mesmo diretório de dados; instâncias
com diretórios diferentes ainda precisam ser evitadas operacionalmente.

1. Encerre a instância antiga. A partir da raiz do projeto, abra o binário
   target/stabilization/debug/discord-server-bot.exe.
2. Confira o PID registrado no startup e garanta uma única instância/deployment
   para o token da aplicação.
3. Execute /profile; verifique SlashDefer reconhecido antes das consultas REST.
4. Cancele uma confirmação: não deve haver alteração do perfil.
5. Abra outra confirmação e confirme: confira ConfirmationAck antes do PATCH.
6. Teste apelido/bio por formulário e PNG pelo parâmetro Attachment. Processamento,
   Data URI e isolamento por guilda continuam usando os serviços existentes.
7. Se 10062 persistir, compare o log de envio inicial e o de falha, usando
   interaction, response_stage, route, interaction_age_ms e PID.
   Não compartilhe tokens ou URLs completas de interação.

Validação: formatação, Clippy, 130 testes e build aprovados. Os testes usam
HTTP local e verificam callback único, defer privado, rota de follow-up,
confirmações, cancelamento, concorrência, formulários, expiração e permissões.
Não alteram perfis reais no Discord.

Referências oficiais: [prazo inicial e follow-ups](https://github.com/discord/discord-api-docs/blob/main/developers/interactions/receiving-and-responding.mdx),
[códigos de erro](https://github.com/discord/discord-api-docs/blob/main/developers/topics/opcodes-and-status-codes.mdx).
