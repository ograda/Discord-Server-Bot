# Implantação V2 de desenvolvimento

A V1 permanece no histórico, com baseline `1808541` em `main` na abertura desta
branch. Não havia tags locais nesse momento. A V2 é incremental na branch
`feat/v2-components-activity`, versão `2.0.0-dev.1`; não representa release estável.

## Bot Rust

Faça backup externo das configurações privadas e de `data/`. Pare a instância
antes de trocar o binário ou de restaurar dados. Não execute duas instâncias
usando a mesma aplicação/estado. Nenhuma operação de homologação real deve ser
disparada pelos testes automatizados.

```powershell
$env:CARGO_TARGET_DIR='target/stabilization'
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Esse diretório de build permite validar enquanto um binário antigo em
`target/debug` está aberto, sem substituir o executável em uso. Pare o bot pelo
seu procedimento normal e inicie o binário escolhido explicitamente para
homologar. Compilar não reinicia o processo automaticamente.

O Docker/Compose principal continua com usuário não privilegiado, filesystem
somente leitura e os mesmos volumes de configuração/dados. Não precisa da
Activity, Node ou portas novas. Com Docker disponível:

```sh
docker compose config --quiet
docker compose build
```

A configuração Compose exige `DISCORD_TOKEN` local. Não publique o resultado
expandido da configuração nem acrescente credenciais aos testes/CI.

## Frontend estático

O frontend é implantado separadamente, conforme [Activity](activity-invite.md).
Node é ferramenta de build, não um processo permanente de produção:

```sh
cd activities/invite
npm ci
npm test
npm run build
```

Hospede somente `dist/` por HTTPS e configure o URL Mapping no Developer Portal.
Ative a configuração por guilda somente quando a aplicação e a conta de teste
forem elegíveis. O CI testa Rust e frontend, mas não faz deploy nem publica
releases. Não altere Public Bot/verificação automaticamente.

## Conversão de Links e retorno à V1

Preview é privado e não converte a publicação. Publish apresenta a confirmação
da conversão e edita a mensagem original por PATCH. Nenhum arquivo de conteúdo
é reordenado ou alterado pela conversão. O JSON mantém Message ID, Channel ID,
dados dos outros módulos e estado operacional.

A flag V2 é irreversível na mensagem convertida. Retornar ao executável V1 não
restaura o formato antigo dessa mensagem, e publicar sobre ela com o construtor
V1 pode ser recusado pelo Discord. Portanto, ensaie primeiro em um quadro/guilda
de homologação. Se precisar voltar ao layout V1, preserve as referências e
planeje uma substituição manual confirmada; não apague a mensagem antecipadamente
nem sobrescreva o JSON com um backup desatualizado enquanto o bot estiver ativo.

Uma falha incerta de escrita remota deixa `pending` registrado. Confira a mensagem
no Discord e reconcilie cuidadosamente antes de liberar outra tentativa. O bot
não repete POST/PATCH cegamente. Um `404/10008` confirmado ao recuperar uma
mensagem apagada pode criar substituta após Publish, arquivando a referência
antiga. Erros de acesso/permissão não são tratados como mensagem apagada.
