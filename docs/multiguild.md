# Configuração multiguild

Uma aplicação atende as guildas registradas em `config/default.toml`. Cada entrada
possui alias, Guild ID, nome da pasta e habilitação:

```toml
[guilds.o-acordo]
id = "123456789012345678"
name = "O Acordo"
enabled = true
```

Não existe `folder`, `servers.toml` ou outra pasta de defaults. `name` é validado
contra caminhos, nomes reservados, separadores e colisões Unicode/Windows. O Guild
ID permanece a chave do estado mesmo que a pasta seja renomeada.

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
      config.toml
      commands.toml
      rules.toml
      announcements.toml
      links.toml
    Outra Comunidade/
      config.toml
      commands.toml
      rules.toml
      announcements.toml
      links.toml
data/guilds/<guild_id>/state.json
```

Os arquivos específicos podem faltar: usam os compartilhados. Tabelas mesclam
campos; listas são substituídas integralmente, inclusive `entries`, `boards` e
cargos moderadores. Um array vazio limpa aquela coleção. Ao alternar Links entre
o formato legado e o de quadros, o arquivo específico define o formato inteiro.
Exemplos estão em `config/guilds/*.toml.example`.

## Flags por raiz e ação

```toml
[commands]
ping = true
role = true
role-all = false # habilite independentemente somente nas guildas desejadas
rules = true
links = true
announcement = true
announce = true
profile = true

[actions.rules]
preview = true
publish = true
update = true
reset = false
remove = true

[actions.profile]
revert = true
nickname = true
bio = true
avatar = true
```

Consulte [defaults completos](../config/guilds/defaults/commands.toml). Ausências
herdam o default; se ambos faltarem, a ação é habilitada respeitando o módulo.
Uma flag de ação não reabilita uma raiz/módulo desabilitado. A ação padrão só roda
se estiver habilitada; caso contrário, escolha outra explicitamente. O registro
por guilda omite raízes desabilitadas, choices desligadas e comandos sem ações disponíveis.
A execução confere novamente essas flags.

Chaves antigas como `"rules.publish"`, `status` e `"announcements.delete"` são
normalizadas por camada em memória. Ações sem equivalente são descontinuadas com
diagnóstico. Arquivos privados não são reescritos. Não são mantidos aliases públicos.
Mudanças de comandos/módulos/canais e do registro exigem reinicialização.

## Rules e Links

As configurações gerais são carregadas no startup. Rules/Links têm um cache
mutável por guilda e `ação:update` explícito: lê o arquivo específico com fallback,
valida e substitui o snapshot somente após sucesso. Preview/Publish usam a memória;
Update não altera o Discord. Arquivo inválido mantém a configuração anterior.
A origem efetiva aparece nos diagnósticos.

`ação:reset` confirma a restauração dos bytes do arquivo em `guilds/defaults` para
`guilds/<nome>`, preserva backup `*.backup-*.bak`, grava atomicamente e recarrega a
memória. Não remove publicações nem seus IDs. Links Add também grava no TOML, com
backup e mutex, validando antes de substituir e preservando o restante do catálogo.
Os caminhos são fixados pelo cadastro, sem parâmetro para apontar outra guilda.

Cada quadro usa Board ID estável. Link ID é único apenas dentro do quadro. Canais
são aliases em `config.toml`, não Message IDs. O formato legado `[[entries]]`
continua aceito como quadro `default`. O JSON guarda referências independentes
por quadro; não troca canal/guilda arbitrariamente durante uma edição.

Links exige `ação`; seu único parâmetro opcional `id` identifica links em Remove.
O quadro é escolhido por menu quando não houver único quadro ou `default`.
Add/Remove e edição de apresentação gravam somente o TOML específico, com backup;
um catálogo herdado é materializado antes da alteração. Remove preserva as outras
entradas e as referências de publicação. `state.link_invites` mantém apenas os
dados operacionais do botão da publicação confirmada da própria guilda. Veja o
[guia de Links](links.md) para o modal e convites.

## JSON e backups

`state.json` permanece por Guild ID. Armazena publicações, revisão, journal,
anúncios gerenciados e personalizações de perfil. Não é a fonte principal de
Rules/Links. Na primeira inicialização, documentos antigos desses módulos são
preservados no backup exato `state.pre-toml-*.json.bak` e em `archived_documents`.
Os IDs de publicação e o perfil são preservados. Um journal pendente deve ser
reconciliado antes de uma migração que alteraria seu estado.

Publique novamente com confirmação após Update/Reset; o bot edita a mensagem
original. Se ela não existe, Publish preserva o ID e informa o problema; Remove
confirmado reconcilia aquele registro antes de uma nova publicação. Um resultado
HTTP incerto mantém `pending`. Não apague o JSON nem repita envios cegamente.

Profile Revert lê `config.toml` específico com fallback por campo para defaults.
Avatar relativo é resolvido na pasta que definiu esse campo, inclusive em defaults.
As personalizações só são limpas após sucesso. O perfil global nunca é alterado.

## Docker e permissões

O Compose mantém filesystem somente leitura, usuário 10001, sem portas,
capabilities ou privilégios adicionais. `/app/config` é somente leitura; a árvore
`/app/config/guilds` permite as gravações necessárias, com o bind mais específico
`/app/config/guilds/defaults` novamente somente leitura. O volume `bot-data` é
separado. Não torne o filesystem inteiro gravável.

Antes de subir no Linux, crie as pastas específicas e permita escrita ao UID 10001
nos diretórios necessários para o rename atômico e backups, por exemplo:

```sh
mkdir -p 'config/guilds/O Acordo'
sudo chown -R 10001:10001 'config/guilds/O Acordo'
```

Repita somente para as guildas registradas. Defaults e registro global precisam
apenas de leitura. Para restringir ainda mais os mounts, substitua a árvore de
guildas por binds individuais das pastas específicas em seu Compose local.
Não use arquivos isolados como bind de escrita: o rename atômico precisa do
diretório. No Docker Desktop, confira também as permissões do compartilhamento.

Faça backup das configurações e do volume JSON com o bot parado. Não publique
backups, estado, `.env` ou configurações privadas. Não execute duas instâncias
compartilhando os mesmos dados. `docker compose config --quiet` evita imprimir token.

## Migração do cadastro legado

Se a instalação ainda usa `config/guilds/<arquivo>.toml`, pare o bot e execute:

```sh
cargo run --bin migrate-config -- config templates
cargo run --bin migrate-config -- --check config
```

A migração valida uma árvore temporária antes de gravar, conserva fontes e cria
`default.toml.pre-multiguild`. Não a execute novamente em instalação já registrada.
O conteúdo/estado JSON tem sua migração separada e automática no startup novo.

## Role-all por guilda e intent global

As raízes são independentes em commands.toml:

```toml
[commands]
role = true
role-all = true

[actions.role-all]
add = true
remove = true
```

Role-all fica desligado por padrão. A intent é uma característica da conexão da
aplicação e fica em `config/default.toml`, fora de qualquer tabela de guilda:

```toml
[intents]
guild_members = true
```

Habilite também Server Members Intent no Developer Portal e reinicie. Não é
necessária para Role individual. Não altere configurações privadas automaticamente
para habilitar essa opção. Cada guilda pode definir `[roles] protected = ["ID"]`;
essa proteção vale para ambos os comandos. Em massa, cargos moderadores e cargos
com Administrator/Manage Guild/Manage Roles também são protegidos.
Os resultados de cada guilda ficam somente em seu próprio `role_operations`.
