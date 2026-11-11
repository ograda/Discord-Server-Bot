# Estabilização V1.1 e perfil por guilda

## Diagnóstico e correções

Anúncios eram persistidos com a chave `announcement-<Interaction ID>`. A confirmação
não informava essa chave, a listagem mostrava dois identificadores sem explicar a
diferença e `edit/delete` rejeitavam Message IDs. Agora a confirmação apresenta a
chave exata, o Message ID e o link público. `list` acrescenta título/resumo, data
derivada do Snowflake e estado local. A existência remota é conferida ao gerenciar.

`edit/delete` oferecem autocomplete por título, chave ou Message ID. Somente IDs
associados a registros da guilda atual são aceitos. Antes do formulário, o bot
recupera a mensagem e confirma autoria, canal e acesso ao histórico. A edição usa
PATCH da mensagem existente; não cria substituta. Se ela foi apagada manualmente,
uma confirmação reconcilia o registro sem republicar. Anúncios antigos continuam
válidos, sem migração do formato de publicações.

Na configuração local examinada, `rules=false` e `links=false` estavam explícitos
e os canais correspondentes estavam ausentes. `commands.toml=true` não sobrepõe um
módulo desabilitado. O filtro Poise estava correto: ambos os grupos eram omitidos.
Após o usuário fornecer os canais, a configuração privada foi atualizada para
habilitar Rules e Links, preservando backup. Profile também está habilitado localmente.
Os IDs reais não aparecem nos exemplos públicos.

O startup agora informa alias/Guild ID, módulos efetivos, comandos registrados,
cada omissão com motivo e sucesso/falha do registro remoto. Para diagnosticar sem
conectar ao Discord:

```sh
cargo run --bin migrate-config -- --check config
```

Grupos com alguma entrada habilitada são registrados parcialmente. Grupos vazios
são omitidos. Regras continuam usando dois embeds #5865F2 com cinco campos cada no
template inicial. Links continuam separados por guilda e limitados a HTTP/HTTPS.
As alterações de conteúdo permanecem em JSON, com atualização da mesma mensagem.

## Configurar Profile

Em `config/guilds/<name>/config.toml`:

```toml
[modules]
profile = true

[profile]
nickname = "Bot da Comunidade"
bio = "Bot oficial da nossa comunidade."
# avatar = "assets/avatar.png"
```

O módulo é opcional e desabilitado por padrão. Arquivos antigos permanecem válidos.
Campos de profile herdam os defaults compartilhados; personalizações feitas por
comandos têm prioridade sobre o TOML efetivo. O arquivo de avatar, mesmo se o caminho
vier dos defaults, é sempre resolvido dentro da pasta da guilda atual.
Não existe `folder`, configuração de username global ou avatar aplicado no startup.

Em `commands.toml`, estas entradas podem ser habilitadas/desabilitadas individualmente:

```toml
[commands]
"profile.show" = true
"profile.nickname" = true
"profile.avatar" = true
"profile.bio" = true
"profile.apply" = true
```

## Comandos e permissões

| Comando | Operação |
|---|---|
| `/profile show` | Consulta apelido/avatar/biografia da guilda, em privado |
| `/profile nickname value` | Define apelido local de 1–32 caracteres |
| `/profile avatar image` | Recebe anexo PNG/JPEG/GIF e altera o avatar da guilda |
| `/profile bio value` | Define biografia de até 190 caracteres, mantendo Markdown |
| `/profile apply` | Aplica somente diferenças do perfil efetivo |

Autor: **Gerenciar servidor** e política administrativa/cargos configurada.
Bot: **Alterar apelido (CHANGE_NICKNAME)** quando houver mudança de apelido.
Avatar e bio usam o perfil do próprio membro, sem solicitar Administrador.
Anúncios/regras/links continuam exigindo autorização de Gerenciar mensagens para o
usuário; o bot precisa Ver canal/Enviar mensagens, Embed Links para embeds e Ler
histórico para recuperar mensagens existentes. Cargos preservam Manage Roles e hierarquia.

Avatares são limitados pela aplicação a 4 MiB e 4096×4096 pixels, com assinatura
de formato e decodificação validada. Caminhos absolutos, `..`, escapes e symlinks
para fora da guilda são rejeitados. Anexos só são baixados do CDN de anexos Discord,
sem redirects, sem enviar o token de bot ao CDN e com limite durante a leitura.
Não há imagem fictícia, diretório temporário público nem upload a outro serviço.

## API e persistência

O adaptador REST tipado está isolado em `services/profile.rs`, usando reqwest.
Serenity 0.12.5 não expõe bio/avatar no builder de membro utilizado pelo projeto.
A atualização usa exclusivamente `PATCH /guilds/{id}/members/@me`, e a consulta
usa o endpoint documentado de membro pelo ID do bot. Nenhum endpoint de usuário
global é modificado. O token é usado em header sensível e não é persistido novamente.

HTTP 400/403 geram mensagens específicas; 429 respeita retry_after em um cooldown
local de perfil, sem reenvio automático. Falhas de transporte pedem conferência do
estado remoto antes de repetir. A API Discord pode aplicar limites adicionais.
O cooldown de perfil é independente do rate limiter dos demais comandos Serenity.

`state.json` ganhou uma propriedade opcional `profile`; estados V1.1 sem ela são
carregados com valores vazios. Não houve regravação dos dados reais durante esta
implementação. Uploads ficam como data URI no JSON privado da guilda, após sucesso
da API. SHA-256 do conteúdo e o hash remoto permitem evitar reenvios de avatar.
O snapshot atômico e o mutex existentes continuam protegendo as alterações.
Nenhum comando altera os TOMLs.

`apply` compara apelido/bio retornados pela API. Caso a consulta não devolva bio,
`show` informa explicitamente essa limitação e mostra a última bio aplicada por
este bot; `apply` utiliza esse último valor confirmado. Mudanças externas de bio
não podem ser detectadas quando o campo não vem na resposta. Avatar usa também
o hash remoto, de modo que uma alteração externa detectável pode ser reaplicada.

Não há uma transação distribuída Discord/disco: se a API aceitar a alteração e a
gravação local falhar, a mensagem explica que o perfil pode já ter sido atualizado.
Confira `/profile show`, corrija o armazenamento e repita conscientemente.
Corpos HTTP, tokens e imagens base64 não são registrados nos logs.

Referências oficiais: [Modify Current Member](https://discord.com/developers/resources/guild#modify-current-member),
[perfis por servidor](https://support.discord.com/hc/en-us/articles/4409388345495-Per-Server-Profiles).

## Respostas e sessões

Respostas administrativas e prévias são efêmeras. Não existe rotina para apagá-las.
Somente a sessão de confirmação expira, desabilitando componentes; a permanência
visual da resposta fica a cargo do Discord. Mensagens públicas e seus identificadores
persistidos não dependem da validade do token de interação.

## Homologação

1. Reinicie o bot atualizado e confira nos logs os grupos rules, links e profile.
2. Execute `/announcements list`; use autocomplete ou Message ID para editar.
   Confirme que a mensagem conserva o ID. Exclua outro anúncio com confirmação.
   Apague um anúncio manualmente e reconcilie pelo comando delete.
3. `/rules preview`, publish e update: dois embeds no modelo inicial e mesmo Message ID.
4. `/links add`, publish, edit e update: valide URL inválida, rascunho e mesmo Message ID.
5. `/profile nickname`: altere o apelido; negue CHANGE_NICKNAME para testar o diagnóstico.
6. `/profile avatar`: envie PNG/JPEG/GIF válido e depois arquivo inválido ou grande.
7. `/profile bio`, show e apply: confirme conteúdo e ausência de alterações repetidas.
8. Reinicie, confira persistência e repita show na segunda guilda: ela deve permanecer
   intacta. Teste também usuário sem autorização e módulos desabilitados.

Os testes automatizados usam arquivos temporários e respostas HTTP simuladas, sem
conectar ao Discord. Homologação de perfil, modals e publicações reais é manual.

## Validação local em 11/11/2026

- `cargo fmt --all -- --check`: aprovado.
- `cargo clippy --all-targets -- -D warnings`: aprovado.
- `cargo test`: 41 testes aprovados, incluindo os 29 anteriores.
- `cargo build`: aprovado.
- `cargo run --bin migrate-config -- --check`: configuração local válida, com
  Rules, Links e Profile habilitados.
- `docker compose config --quiet`: aprovado (exit 0), com aviso de acesso negado
  à configuração de credenciais local do Docker.
- `docker compose build`: tentado, bloqueado pelo credential helper do Docker
  (`A specified logon session does not exist`). A imagem não foi validada.

O executável anterior estava em execução e bloqueou a primeira tentativa de build
no Windows. Os comandos Cargo foram concluídos com
`$env:CARGO_TARGET_DIR='target/stabilization'`, sem encerrar o bot em uso.
Encerre a instância antiga antes de iniciar a nova; não execute duas instâncias
compartilhando o mesmo diretório de dados. O novo registro remoto e os efeitos dos
comandos só serão homologados depois dessa reinicialização.
