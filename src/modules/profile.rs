use crate::{
    core::{Context, Error, permissions},
    modules::actions::ProfileAction,
    services::{
        profile::{self as service, Patch},
        ui,
    },
};
use anyhow::{Result, ensure};
use poise::serenity_prelude as s;

async fn authorize(ctx: Context<'_>) -> Result<permissions::Authority> {
    let cfg = permissions::guild_config(ctx)?;
    ensure!(
        cfg.modules
            .unwrap_or(ctx.data().config.global.modules)
            .profile,
        "Módulo profile desabilitado"
    );
    permissions::administrative(ctx, s::Permissions::MANAGE_GUILD).await
}
#[derive(poise::Modal)]
#[name = "Personalizar perfil da guilda"]
struct TextForm {
    #[name = "Novo valor"]
    #[paragraph]
    #[max_length = 190]
    value: String,
}
/// Perfil somente desta guilda. Sem ação, confirma restauração dos valores TOML.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn profile(
    ctx: Context<'_>,
    #[description = "Ação (padrão: revert, com confirmação)"] ação: Option<ProfileAction>,
    #[description = "Novo apelido (1–32 caracteres)"] apelido: Option<String>,
    #[description = "Nova biografia (até 190 caracteres)"] biografia: Option<String>,
    #[description = "Imagem para ação avatar"] imagem: Option<s::Attachment>,
    #[description = "Converter explicitamente para PNG 512×512, sem distorcer"] normalizar: Option<
        bool,
    >,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    let action = cfg.select_action(
        "profile",
        ação.map(|v| v.as_str()),
        ctx.data().config.global.modules,
    )?;
    let supplied: Vec<_> = [
        ("apelido", apelido.is_some()),
        ("biografia", biografia.is_some()),
        ("imagem", imagem.is_some()),
        ("normalizar", normalizar.is_some()),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.then_some(k))
    .collect();
    permissions::conditional_parameters("profile", &action, &supplied)?;
    permissions::action(ctx, "profile", &action)?;
    let auth = authorize(ctx).await?;
    let desired = match action.as_str() {
        "revert" => {
            let config = if ctx.data().config.global.guilds.is_some() {
                service::load_defaults(&ctx.data().config.root, cfg)?
            } else {
                service::ProfileConfig {
                    nickname: cfg.profile.nickname.clone(),
                    bio: cfg.profile.bio.clone(),
                    avatar_data: cfg.profile.avatar_data.clone(),
                    avatar: None,
                }
            };
            if !ui::confirm(ctx, "Restaurar apelido, biografia e avatar do TOML nesta guilda? Valores ausentes serão removidos do perfil da guilda. As personalizações JSON serão limpas somente após sucesso. O perfil global não será alterado.", "Restaurar perfil", None).await? { return Ok(()); }
            service::revert(&config)
        }
        "nickname" | "bio" => {
            let value = if let Some(value) = if action == "nickname" {
                apelido
            } else {
                biografia
            } {
                value
            } else {
                let Some(form) = ui::open_form(
                    ctx,
                    TextForm {
                        value: String::new(),
                    },
                )
                .await?
                else {
                    return Ok(());
                };
                form.value
            };
            if action == "nickname" {
                service::validate_nickname(&value)?;
                Patch {
                    nick: Some(value),
                    ..Default::default()
                }
            } else {
                service::validate_bio(&value)?;
                Patch {
                    bio: Some(value),
                    ..Default::default()
                }
            }
        }
        "avatar" => {
            let image = imagem.ok_or_else(|| anyhow::anyhow!("A ação avatar exige o parâmetro imagem do tipo Attachment. Execute /profile ação:avatar e selecione o arquivo em imagem."))?;
            tracing::info!(guild = %auth.guild.id, module = "profile", action = "avatar", stage = "attachment", bytes = image.size, width = image.width, height = image.height, "Anexo resolvido pelo Serenity");
            let prepared = ctx
                .data()
                .profile_client
                .attachment(&image, normalizar.unwrap_or(false))
                .await?;
            ctx.say(format!("Imagem validada: {}×{}, {} bytes de saída, {}. {}", prepared.width, prepared.height, prepared.output_bytes, prepared.mime,
                if prepared.normalized { "Normalização solicitada: PNG 512×512 com proporção preservada e margens transparentes; GIF torna-se estático." } else { "Dimensões e formato preservados." })).await?;
            Patch {
                avatar: Some(prepared.data),
                ..Default::default()
            }
        }
        _ => unreachable!(),
    };
    apply(ctx, &action, desired).await
}
async fn apply(ctx: Context<'_>, action: &str, desired: Patch) -> Result<(), Error> {
    permissions::action(ctx, "profile", action)?;
    let auth = authorize(ctx).await?;
    if desired.nick.is_some() || desired.clear_nick {
        permissions::ensure_bot_permissions(&auth, s::Permissions::CHANGE_NICKNAME)?;
    }
    let changed = service::apply(
        &ctx.data().profile_client,
        ctx.http().token(),
        &ctx.data().store,
        service::Operation {
            guild: auth.guild.id,
            bot: auth.bot.user.id,
            desired,
            revert: action == "revert",
        },
    )
    .await?;
    tracing::info!(guild = %auth.guild.id, module = "profile", action, stage = "complete", changed, "Perfil da guilda aplicado e estado salvo");
    ctx.say(if action == "revert" {
        "Perfil TOML restaurado nesta guilda; personalizações JSON limpas após sucesso."
    } else if changed {
        "Perfil desta guilda atualizado e personalização salva."
    } else {
        "Perfil já corresponde ao solicitado; personalização salva nesta guilda."
    })
    .await?;
    Ok(())
}
