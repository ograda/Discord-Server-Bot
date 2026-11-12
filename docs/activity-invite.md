# Activity de convites — V2 em desenvolvimento

Esta entrega prepara a integração oficial. Compilação e testes locais não
constituem homologação no Discord. Nenhuma Activity foi habilitada nas
configurações privadas, nenhuma aplicação foi criada e o Developer Portal não
foi alterado automaticamente.

## Disponibilidade e bot privado

Activities não verificadas são limitadas a servidores com **menos de 25
membros** e a desenvolvedores/testadores autorizados. O servidor com cerca de
500 membros deve continuar com a Activity desabilitada. Disponibilidade ampla
exige atender aos requisitos de verificação do Discord.
[Restrições oficiais](https://support-dev.discord.com/hc/en-us/articles/26576097154199-What-are-Verified-and-Unverified-Activities).

O Discord informa que, após verificar a aplicação, não é possível desligar
Public Bot. Isso precisa ser avaliado pelo proprietário antes de qualquer
verificação; este código não altera essa opção nem inicia verificação.
[Public/Private Bot](https://support-dev.discord.com/hc/en-us/articles/21204493235991-How-Can-Users-Discover-and-Play-My-Activity).

O recurso é independente do processo Rust e fica desligado por padrão. Para
uma guilda de homologação elegível, acrescente ao seu `config.toml` específico:

```toml
[activities.invite]
enabled = true
```

Reinicie o bot para carregar essa configuração. Para desligar, use `false` e
reinicie. Não há hot reload de configuração geral. O restante do bot e Copiar
convite continuam funcionando sem a Activity ou sua hospedagem. O módulo e o
comando Links também precisam estar habilitados para seus botões públicos.

## Frontend e SDK

`activities/invite` contém TypeScript, Vite, HTML/CSS e o SDK oficial. Não há
backend, OAuth com client secret, armazenamento de dados ou servidor Node de
produção. Somente o Application ID público está no frontend. Nunca inclua token
do bot, client secret ou credenciais em variáveis `VITE_*`.

O controlador espera `DiscordSDK.ready()`, exige `guildId` e `channelId`, habilita
o botão e executa `commands.openInviteDialog()` mediante clique. Não abre o
diálogo automaticamente: mantém o gesto explícito do usuário e evita pop-ups
inesperados. Há timeout de inicialização, bloqueio de cliques simultâneos e
diagnósticos sem despejar erros internos do cliente.

O diálogo é nativo do contexto da Activity, não recebe o convite permanente
de `links.toml` e pode apresentar outro convite. Não funciona em DMs; exige as
permissões de convite do canal. O comando não exige scopes OAuth adicionais;
por isso não adicionamos um fluxo de autenticação ou backend apenas para ele.
[SDK](https://docs.discord.com/developers/developer-tools/embedded-app-sdk#openinvitedialog),
[contexto e permissões](https://docs.discord.com/developers/activities/development-guides/user-actions#open-invite-dialog).

## Desenvolvimento no Windows

Utilize Node 22.12+ da linha 22, ou uma versão LTS compatível indicada em
`package.json`. Dentro do diretório do frontend:

```powershell
cd activities/invite
Copy-Item .env.example .env
# Edite .env: VITE_DISCORD_CLIENT_ID=<Application ID do mesmo bot>
npm install
npm run dev
```

O lockfile está versionado. Para instalações reproduzíveis/CI, use `npm ci`.
`npm test` testa o controlador com SDK simulado; `npm run build` verifica
TypeScript e produz `dist/`. Não são necessárias credenciais para esses testes.
Em navegador comum, `http://127.0.0.1:5173/?demo=1` exercita a apresentação em
desenvolvimento, sem chamar o Discord. O modo demo não entra em produção.

Em outro terminal, execute o túnel instalado pelo operador:

```powershell
cloudflared tunnel --url http://127.0.0.1:5173
```

Se o túnel encaminhar seu hostname no cabeçalho Host, encerre apenas o Vite,
defina `$env:ACTIVITY_TUNNEL_HOST='seu-tunel.trycloudflare.com'` e execute
`npm run dev` novamente nesse terminal. Essa variável é lida no início do Vite,
não vai para o bundle e não libera hostnames arbitrários. A URL do túnel deve
ser HTTPS. Alterar o hostname exige atualizar o Portal e reiniciar o Vite.
[Guia oficial de desenvolvimento local](https://docs.discord.com/developers/activities/development-guides/local-development).

## Developer Portal — configuração manual

1. Abra a **mesma aplicação** usada pelo bot e confira seu Application ID.
2. Em Activities → Settings, habilite Activities/Embedded para essa aplicação.
   Não ative publicação/discovery/verificação nem altere Public Bot neste teste.
3. Em Activities → URL Mappings, associe o prefixo `/` ao hostname HTTPS do
   frontend/túnel, sem `https://` no campo Target. Salve. O Vite usa base `/`.
4. Confira o entry point padrão criado pelo Discord para a Activity e os
   recursos/plataformas exigidos pelo Portal. O registro slash por guilda do
   bot não modifica esse entry point global. Não copie tokens para o frontend.
5. Adicione a conta de teste em Developer Team ou App Testers e aceite o convite.
   No cliente de teste, habilite o modo de desenvolvimento/teste de Activities
   e selecione a aplicação quando necessário. Use uma guilda com menos de 25 membros.
6. Confirme as permissões de usar aplicativos/Activities e criar convites no
   canal usado. Não amplie permissões de usuários para contornar um erro.
7. Habilite `[activities.invite]` somente nessa guilda, reinicie o bot e use o
   botão do quadro de Links. A resposta inicial do botão é `LAUNCH_ACTIVITY`
   (tipo 12), não um link externo nem uma mensagem pública criada pelo bot.

Os nomes das telas podem variar com o Portal. Confira o
[tutorial oficial](https://docs.discord.com/developers/activities/building-an-activity)
ao homologar. Não é necessário criar uma segunda aplicação nesta entrega.

## Hospedagem

Publique exclusivamente `activities/invite/dist/` em Cloudflare Pages ou em um
servidor HTTPS estático na VPS. Configure `VITE_DISCORD_CLIENT_ID` no build e
`npm ci && npm run build` como comando de compilação. Não envie `.env` privado,
`node_modules`, dados do bot ou binários Rust ao host do frontend.

Atualize o URL Mapping `/` para o hostname final. Não use cabeçalhos próprios
que proíbam o iframe do Discord (`X-Frame-Options: DENY/SAMEORIGIN`); ajuste a
política de enquadramento ao ambiente oficial. O SDK e CSS são empacotados, sem
CDNs externos. O Compose principal permanece independente, sem novas portas ou
um processo Node permanente. Seu Dockerfile passa a incluir somente a fonte
vendorizada necessária ao build Rust.

## Homologação

Siga primeiro [Links](links.md) para converter uma publicação após confirmação
e conferir o mesmo Message ID. Com Activity desabilitada, Convidar amigos deve
exibir privadamente o aviso provisório exato; Copiar convite deve continuar
respondendo com o convite configurado, inclusive após reiniciar o bot.

Na guilda pequena habilitada, confira lançamento da Activity, identidade da
aplicação, carregamento do SDK e clique em **Abrir convites do Discord**. Teste
canal de texto/voz apropriado, cliente web/desktop e conta sem permissão de
convite. Contextos ausentes, cliente incompatível ou permissões negadas devem
mostrar diagnóstico claro. A conta comum elegível não precisa ser administradora
do bot; o Discord continua aplicando suas próprias restrições de testadores.

Desabilite novamente o recurso e teste duas guildas com configurações distintas.
Verifique ausência de alterações cruzadas, mensagens públicas adicionais e
URLs abertas automaticamente. Registre a evidência real separadamente em
[validação V2](validation-v2.md). Não considere o servidor de 500 membros homologado.
