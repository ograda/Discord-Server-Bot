# Cargos — Role individual e Role-all

`/role` e `/role-all` são comandos raiz independentes, sem subcomandos.
Ambos exigem a choice `ação:add` ou `ação:remove` e um seletor nativo `cargo` ROLE.
Somente `/role` tem `usuário`, obrigatório e do tipo USER. Não é texto, não aceita
o identificador especial `all` e não exige copiar IDs de usuários.

```text
/role ação:add usuário:@Pessoa cargo:@Participante
/role ação:remove usuário:@Pessoa cargo:@Participante
/role-all ação:add cargo:@Participante
/role-all ação:remove cargo:@Participante
```

## Habilitação

No `commands.toml` da guilda, as raízes são independentes:

```toml
[commands]
role = true
role-all = true

[actions.role]
add = true
remove = true

[actions.role-all]
add = true
remove = true
```

O módulo `roles` e a administração devem estar ativos. Role-all fica desligado
por padrão e exige habilitação explícita; não herda o opt-in de Role. As choices
desabilitadas são omitidas por guilda e verificadas novamente no runtime.

Operações em massa requerem a lista completa de membros. Configure no TOML global
`config/default.toml` (tabela raiz, fora de `[guilds.*]`):

```toml
[intents]
guild_members = true
```

Habilite **Server Members Intent** em Bot → Privileged Gateway Intents no Developer
Portal, obtenha aprovação quando exigida pelo Discord e reinicie o processo.
Essa opção adiciona GUILD_MEMBERS à conexão. Uma intent solicitada sem habilitação
no Portal pode fazer o Discord recusar a conexão. Não habilite Presence ou Message
Content para esta funcionalidade.

Sem esse opt-in, a conexão continua somente com GUILDS e `/role` funciona por
consulta REST individual. `/role-all`, se registrado na guilda, retorna um
diagnóstico da configuração ausente antes de listar ou modificar membros.

Referências oficiais: [List Guild Members](https://github.com/discord/discord-api-docs/blob/main/developers/resources/guild.mdx#list-guild-members)
e [Privileged Intents](https://github.com/discord/discord-api-docs/blob/main/developers/events/gateway.mdx#privileged-intents).

## Autorização e cargos protegidos

Role exige Gerenciar Cargos. Role-all exige **Gerenciar Cargos e Gerenciar
Servidor**, além do cargo moderador configurado quando houver essa restrição.
O dono da guilda dispensa suas restrições pessoais; as permissões e a hierarquia
do bot continuam obrigatórias.

As duas operações bloqueiam @everyone, cargos gerenciados ou de integração/bot,
boost, assinatura/compra e cargos vinculados. O cargo precisa pertencer à guilda e
estar abaixo do maior cargo do bot e do administrador (exceto o dono).

Proteções adicionais podem ser configuradas por guilda:

```toml
[roles]
moderators = ["111111111111111111"]
protected = ["222222222222222222"]
```

`protected` impede alterações por ambos os comandos. Em massa, também são
bloqueados os cargos de `moderators` e cargos com Administrator, Manage Guild ou
Manage Roles. Não remova essas proteções para distribuir permissões administrativas
indiscriminadamente.

Role valida a associação do usuário à guilda por REST. Dono e próprio bot são
protegidos; a hierarquia do alvo também é verificada. Se o usuário já está no
estado desejado, informa que nenhuma alteração é necessária.

## Confirmação e execução em massa

1. Reserva a guilda contra outra operação de cargos, inclusive individual.
2. Verifica autorização e cargo e lista todos os membros por REST, em páginas
   de até 1000 com cursor `after`. Não usa uma lista parcial do cache.
3. Ignora bots, dono, autor, próprio bot, membros em triagem, estado já desejado
   e alvos fora da hierarquia. Papéis de membro desconhecidos são tratados como
   inelegíveis. Falha ou paginação inconsistente aborta antes da confirmação.
4. Informa a quantidade de humanos elegíveis e ignorados e apresenta confirmação
   vinculada ao autor, com expiração. Cancelar/expirar não modifica cargos.
5. Depois de confirmar, registra a operação e processa apenas os IDs confirmados,
   sequencialmente e com awaits. Membros que chegaram depois não ampliam o escopo.
6. Reconsulta permissões, cargo e alvo antes de cada alteração. Bots ou membros
   cujo estado/hierarquia mudou são ignorados. Saídas da guilda são contadas.
7. Usa PUT/DELETE do cargo de um membro, sem substituir os outros cargos. O cliente
   padrão do Serenity respeita buckets, limites globais e respostas 429.
8. Registra cada sucesso, falha ou motivo de ignorar. Perda de autorização, erro
   geral/403/429 não recuperado ou resultado de transporte incerto interrompe novas
   alterações. Uma falha individual definitiva, como 400, é contada e permite
   continuar nos demais. Não há reenvio cego nem reversão automática dos sucessos.
9. Apresenta sucessos, falhas, ignorados e não processados. O lock da guilda é
   liberado na conclusão, cancelamento ou erro; outras guildas são independentes.

A autorização e a hierarquia ainda podem mudar entre a última consulta e o pedido
HTTP. A API pode rejeitar a alteração; essa falha faz parte do resumo, sem alegar
uma transação atômica de toda a guilda.

## Progresso, histórico e recuperação

Antes da primeira alteração, salva `role_operations.<operação>` em
`data/guilds/<Guild ID>/state.json`. Preserva publicações, perfis e os demais dados.
Há checkpoints a cada 25 candidatos e resumo final com status, contadores e motivo
de interrupção. Os logs contêm IDs e resultados, sem tokens ou credenciais.

A resposta efêmera é atualizada a cada 30 segundos enquanto o token permite.
Trabalhos muito longos podem ultrapassar a validade da interação; continuam
processando os alvos já confirmados, e o resumo permanece no JSON e nos logs.
Falha de entrega da resposta não repete nem desfaz operações.

Não há retomada automática após reinício. Uma parada abrupta pode deixar status
`running` no último checkpoint; trate-o como resultado incompleto/incerto e confira
o estado real antes de repetir. Uma nova execução sempre exige nova contagem e
confirmação e ignora quem já estiver no estado desejado. Se o armazenamento falhar
antes de registrar a operação, nenhum cargo é alterado. Falhas posteriores são
diagnosticadas, preservando os contadores disponíveis nos logs/resposta.

## Homologação

Use uma guilda de desenvolvimento com humanos e bots. Teste Add/Remove individual
sem intent, acesso negado, usuário ausente e cargo acima do bot/autor. Habilite o
opt-in global e por guilda para testar contagem, cancelar/expirar, confirmação,
estado já desejado, proteção de bots/admins e resumo parcial. Durante uma
confirmação/operação, outra chamada de cargos da mesma guilda deve ser recusada;
uma segunda guilda deve continuar independente. A validação real é manual;
os testes automatizados não utilizam credenciais reais nem alteram cargos reais.

## Validação local

Foram executados `cargo fmt --all -- --check`, Clippy com `-D warnings`, `cargo test`
e `cargo build`: aprovados, com 78 testes ao todo. Os 14 testes específicos de
cargos incluem paginação acima de 1000 membros, seletores USER/ROLE, flags
independentes, autorização reforçada, proteções, idempotência, erros parciais,
interrupção por perda de autorização/429 e isolamento/concorrência por guilda.
Um teste HTTP local verifica o adaptador REST do Serenity e os endpoints por
membro. Nenhum token real foi utilizado. A compilação utilizou
`CARGO_TARGET_DIR=target/stabilization`, sem interromper o bot em execução.
O registro remoto e a homologação de cargos reais ficam para a reinicialização
e os testes manuais posteriores.
