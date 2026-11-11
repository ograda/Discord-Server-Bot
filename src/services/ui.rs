use crate::core::{Context, Error};
use anyhow::ensure;
use poise::serenity_prelude as s;
use std::time::Duration;

pub async fn confirm(
    ctx: Context<'_>,
    prompt: &str,
    label: &str,
    content: Option<(&super::content::Document, &str)>,
) -> Result<bool, Error> {
    let confirm = format!("confirm:{}", ctx.id());
    let cancel = format!("cancel:{}", ctx.id());
    let mut reply = poise::CreateReply::default()
        .content(prompt)
        .ephemeral(true)
        .allowed_mentions(s::CreateAllowedMentions::new())
        .components(vec![s::CreateActionRow::Buttons(vec![
            s::CreateButton::new(&confirm)
                .label(label)
                .style(s::ButtonStyle::Primary),
            s::CreateButton::new(&cancel).label("Cancelar"),
        ])]);
    if let Some((document, kind)) = content {
        reply.embeds = document.embeds(kind)?;
        if !document.text.is_empty() {
            reply = reply.content(&document.text);
        }
    }
    let reply = ctx.send(reply).await?;
    let message = reply.message().await?;
    let interaction = message
        .await_component_interaction(ctx.serenity_context())
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(120))
        .await;
    let confirmed = if let Some(interaction) = interaction {
        interaction
            .create_response(ctx.http(), s::CreateInteractionResponse::Acknowledge)
            .await?;
        interaction.data.custom_id == confirm
    } else {
        false
    };
    // Consume once and disable both controls before any side effect.
    reply
        .edit(ctx, poise::CreateReply::default().components(vec![]))
        .await?;
    if !confirmed {
        ctx.say("Operação cancelada ou expirada; nada alterado.")
            .await?;
    }
    Ok(confirmed)
}

pub async fn select(
    ctx: Context<'_>,
    prompt: &str,
    options: &[(String, String)],
) -> Result<Option<String>, Error> {
    ensure!(
        !options.is_empty(),
        "Nenhum recurso disponível nesta guilda"
    );
    if options.len() == 1 {
        return Ok(Some(options[0].0.clone()));
    }
    let select = format!("select:{}", ctx.id());
    let previous = format!("previous:{}", ctx.id());
    let next = format!("next:{}", ctx.id());
    let cancel = format!("selection-cancel:{}", ctx.id());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut page = 0;
    loop {
        let choices = options
            .iter()
            .skip(page * 25)
            .take(25)
            .map(|(id, title)| {
                s::CreateSelectMenuOption::new(title.chars().take(100).collect::<String>(), id)
            })
            .collect();
        let reply = ctx
            .send(
                poise::CreateReply::default()
                    .content(prompt)
                    .ephemeral(true)
                    .components(vec![
                        s::CreateActionRow::SelectMenu(s::CreateSelectMenu::new(
                            &select,
                            s::CreateSelectMenuKind::String { options: choices },
                        )),
                        s::CreateActionRow::Buttons(vec![
                            s::CreateButton::new(&previous)
                                .label("Anterior")
                                .disabled(page == 0),
                            s::CreateButton::new(&next)
                                .label("Próxima")
                                .disabled((page + 1) * 25 >= options.len()),
                            s::CreateButton::new(&cancel).label("Cancelar"),
                        ]),
                    ]),
            )
            .await?;
        let message = reply.message().await?;
        let interaction = message
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await;
        if let Some(interaction) = &interaction {
            interaction
                .create_response(ctx.http(), s::CreateInteractionResponse::Acknowledge)
                .await?;
        }
        reply
            .edit(ctx, poise::CreateReply::default().components(vec![]))
            .await?;
        let Some(interaction) = interaction else {
            return Ok(None);
        };
        if interaction.data.custom_id == select {
            if let s::ComponentInteractionDataKind::StringSelect { values } = interaction.data.kind
                && let Some(id) = values
                    .first()
                    .filter(|id| options.iter().any(|(key, _)| key == *id))
            {
                return Ok(Some(id.clone()));
            }
            return Ok(None);
        }
        if interaction.data.custom_id == next && (page + 1) * 25 < options.len() {
            page += 1;
        } else if interaction.data.custom_id == previous && page > 0 {
            page -= 1;
        } else {
            return Ok(None);
        }
    }
}

pub async fn open_form<M: poise::Modal + Send + Sync>(
    ctx: Context<'_>,
    form: M,
) -> Result<Option<M>, Error> {
    let custom_id = format!("form:{}", ctx.id());
    let reply = ctx
        .send(
            poise::CreateReply::default()
                .content("Abra o formulário para preencher os dados (90 segundos).")
                .ephemeral(true)
                .components(vec![s::CreateActionRow::Buttons(vec![
                    s::CreateButton::new(&custom_id).label("Preencher formulário"),
                ])]),
        )
        .await?;
    let message = reply.message().await?;
    let interaction = message
        .await_component_interaction(ctx.serenity_context())
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(90))
        .await;
    let result = if let Some(interaction) = interaction {
        poise::execute_modal_on_component_interaction(
            ctx,
            interaction,
            Some(form),
            Some(Duration::from_secs(180)),
        )
        .await?
    } else {
        None
    };
    reply
        .edit(ctx, poise::CreateReply::default().components(vec![]))
        .await?;
    if result.is_none() {
        ctx.say("Formulário cancelado ou expirado; nada alterado.")
            .await?;
    }
    Ok(result)
}
