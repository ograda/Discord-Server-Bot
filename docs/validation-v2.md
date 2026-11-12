# Homologação V2 — Components V2 e Activity

## Validação local executada em 12/11/2026

| Verificação | Resultado |
|---|---|
| `cargo fmt --all -- --check` | Aprovado |
| `cargo clippy --all-targets -- -D warnings` | Aprovado, sem avisos |
| `cargo test` | 143 testes aprovados, sem credenciais reais |
| `cargo build` | Aprovado no Windows, em `target/stabilization` |
| `npm ci` | Aprovado pelo lockfile da Activity |
| `npm test` | 5 testes aprovados com SDK simulado |
| `npm run build` | TypeScript e build estático aprovados |
| `docker compose config --quiet` | Aprovado |
| `docker compose build` | Imagem Linux construída; processo do bot não iniciado |

Serenity 0.12.5 e Poise 0.6.2 preservados, sem patches locais. A imagem Docker
validada não contém o frontend, que tem implantação estática independente.
O Developer Portal e as configurações privadas não foram alterados. Os roteiros
abaixo permanecem **pendentes de execução real no Discord**, incluindo migração
visual, diálogos nativos, elegibilidade e funcionamento para usuários finais.

Os testes automatizados não usam credenciais reais nem validam a renderização
ou a elegibilidade no cliente Discord. Este roteiro deve ser executado pelo
operador após preparar o ambiente de [Activity](activity-invite.md).

## Links e migração

1. Faça backup dos TOMLs/JSON e confirme a versão do binário e uma única instância.
2. Comece com Activity desabilitada. Use `/links ação:preview` com quadro único,
   `default` e múltiplos quadros. Confira um Container, cor configurada, título,
   introdução, links Markdown/labels, quebras internas e convite por último.
3. Confira thumbnail do ícone da guilda, ausência de thumbnail sem ícone,
   Separator e os dois botões dentro do Container. Nenhum embed extra deve existir.
4. Abra Publish em uma mensagem V1. Confira o aviso de conversão irreversível;
   cancele e confirme que a mensagem/IDs continuam intactos. Abra novamente,
   confirme Publicar e compare Channel ID/Message ID antes e depois.
5. Edite/repita Publish no quadro já V2: deve usar o mesmo Message ID. Clique
   duas vezes em confirmar; não deve haver publicação duplicada. Teste uma
   alteração concorrente de TOML/estado e confira a recusa de prévia obsoleta.
6. Teste Add (modal de cinco campos), Salvar/Cancelar, Remove por ID/menu,
   Update e Reset. Confira backups, comentários e isolamento dos outros quadros.
   Essas ações não alteram automaticamente a publicação pública.
7. Como membro comum, use Copiar convite e confira URL privada em bloco de
   código. Mude o TOML + Update: o convite da publicação deve continuar o
   anterior até Publish. Reinicie o bot e repita ambos os botões públicos.
8. Com Activity desabilitada, Convidar amigos mostra somente para quem clicou:
   `Lembra o besta do Ograda que essa funcionalidade ainda não está disponível`.
   O link Markdown continua abrindo o convite configurado.
9. Em um quadro descartável, teste mensagem realmente apagada (`404/10008`):
   Publish confirmado cria substituta e arquiva a referência anterior. Falhas
   de permissão/acesso ou PATCH de migração não podem disparar outra publicação.
10. Teste duas guildas e confirme que conteúdo/ID/ícone/configuração/estado de
    uma não mudam na outra. Faça uma verificação dos comandos V1 preservados.

## Activity — somente servidor elegível

1. Use guilda com menos de 25 membros e conta desenvolvedora/testadora autorizada.
   Não habilite no servidor de aproximadamente 500 membros nesta fase.
2. Configure a mesma aplicação, URL Mapping `/`, HTTPS e Application ID público.
3. Habilite `activities.invite.enabled` somente na guilda pequena e reinicie.
4. Clique Convidar amigos: confirme lançamento da aplicação sem redirecionamento
   externo nem mensagem pública adicional do bot. Verifique o carregamento do SDK.
5. Clique Abrir convites do Discord: confira a interface nativa no contexto
   atual. Ela pode oferecer um convite diferente do permanente do TOML.
6. Teste falta de permissão para criar convites, contexto sem guilda/canal,
   frontend indisponível e cliente incompatível. Registre a mensagem exibida.
   Rejeição definitiva do callback permite aviso privado; timeout, 10062 ou
   interação já respondida não devem provocar outra resposta inicial.
7. Desabilite/reinicie e confira fallback e Copiar convite novamente.

## Evidências

Registre data, commit/binário, cliente/plataforma, guilda de teste, tamanho do
servidor, status de testador, Message IDs antes/depois e resultados de cada etapa.
Não inclua token, parâmetros de interação, convites privados ou dados pessoais
em um relatório público. A release estável 2.0 só deve ser considerada após
homologação real e decisão explícita do proprietário.
