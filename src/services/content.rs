use anyhow::{Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Document {
    pub title: String,
    pub description: String,
    pub color: u32,
    pub thumbnail: String,
    pub image: String,
    pub footer: String,
    pub entries: Vec<Entry>,
    pub text: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub emoji: String,
    pub url: String,
    pub order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<LinkKind>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    DiscordInvite,
}

impl Entry {
    pub fn is_discord_invite(&self) -> Result<bool> {
        valid_url(&self.url)?;
        let invite = discord_invite_url(&self.url);
        ensure!(
            self.kind != Some(LinkKind::DiscordInvite) || invite,
            "Link {}: kind=discord_invite exige uma URL oficial de convite Discord válida",
            self.id
        );
        Ok(invite)
    }
    pub fn validate_link(&self) -> Result<()> {
        ensure!(
            crate::config::registry::valid_alias(&self.id),
            "ID do link inválido"
        );
        ensure!(!self.title.trim().is_empty(), "Título do link obrigatório");
        self.is_discord_invite()?;
        if let Some(label) = &self.label {
            ensure!(
                !label.trim().is_empty() && !label.chars().any(char::is_control),
                "Legenda do link deve ser preenchida e não conter controles"
            );
            limit(label, 256)?;
        }
        Ok(())
    }
}
pub fn discord_invite_url(value: &str) -> bool {
    if valid_url(value).is_err() {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    // Keep optional tracking parameters, but never permit code-fence injection in copy responses.
    if url.port().is_some() || value.contains('`') {
        return false;
    }
    let path = url.path().strip_prefix('/').unwrap_or_default();
    let parts: Vec<_> = path.strip_suffix('/').unwrap_or(path).split('/').collect();
    let code = match (url.host_str(), parts.as_slice()) {
        (Some("discord.gg"), [code]) => *code,
        (Some("discord.com"), ["invite", code]) => *code,
        _ => return false,
    };
    !code.is_empty()
        && code
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
pub fn valid_url(value: &str) -> Result<()> {
    ensure!(
        value.len() <= 2048 && !value.chars().any(char::is_whitespace),
        "URL vazia, longa ou malformada"
    );
    let url = url::Url::parse(value)?;
    ensure!(
        ["http", "https"].contains(&url.scheme())
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "URL deve ser HTTP/HTTPS válida, sem credenciais"
    );
    Ok(())
}
fn limit(value: &str, max: usize) -> Result<()> {
    ensure!(
        value.encode_utf16().count() <= max,
        "Conteúdo excede limite de {max} caracteres"
    );
    Ok(())
}
impl Document {
    pub fn embeds(&self, kind: &str) -> Result<Vec<s::CreateEmbed>> {
        limit(&self.text, 2000)?;
        limit(&self.title, 256)?;
        limit(&self.description, 4096)?;
        if kind != "rules" {
            limit(&self.footer, 2048)?;
        }
        ensure!(self.color <= 0xffffff, "Cor inválida");
        for url in [&self.thumbnail, &self.image] {
            if !url.is_empty() {
                valid_url(url)?;
            }
        }
        let mut entries = self.entries.clone();
        entries.sort_by_key(|e| e.order);
        let mut ids = std::collections::HashSet::new();
        for entry in &entries {
            ensure!(
                crate::config::registry::valid_alias(&entry.id),
                "ID de seção inválido"
            );
            ensure!(
                !entry.id.is_empty() && ids.insert(&entry.id),
                "Identificador vazio ou duplicado"
            );
            ensure!(
                !entry.title.trim().is_empty(),
                "Título da seção obrigatório"
            );
            if kind == "links" {
                valid_url(&entry.url)?;
            }
        }
        if kind == "rules" {
            return self.rules_embed(&entries);
        }
        if kind == "links" {
            return self.links_embed(&entries);
        }
        let fields_per_embed = 5;
        let count = entries.len().div_ceil(fields_per_embed).max(1);
        ensure!(count <= 10, "Máximo de 50 seções / 10 embeds");
        if self.title.is_empty() && self.description.is_empty() && entries.is_empty() {
            ensure!(!self.text.trim().is_empty(), "Mensagem vazia");
            return Ok(vec![]);
        }
        let mut embeds = Vec::new();
        let mut total = 0;
        for index in 0..count {
            let mut embed = s::CreateEmbed::new().color(self.color);
            if index == 0 {
                embed = embed.title(&self.title).description(&self.description);
                total +=
                    self.title.encode_utf16().count() + self.description.encode_utf16().count();
                if !self.thumbnail.is_empty() {
                    embed = embed.thumbnail(&self.thumbnail);
                }
                if !self.image.is_empty() {
                    embed = embed.image(&self.image);
                }
            }
            for (offset, entry) in entries
                .iter()
                .skip(index * fields_per_embed)
                .take(fields_per_embed)
                .enumerate()
            {
                let number = index * fields_per_embed + offset + 1;
                let prefix = if kind == "rules" {
                    ["1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"]
                        .get(number - 1)
                        .map_or_else(|| format!("{number}."), |v| v.to_string())
                } else {
                    String::new()
                };
                let title = format!("{prefix} {} {}", entry.emoji, entry.title);
                let description = if kind == "links" {
                    format!(
                        "[Abrir link](<{}>)\n{}",
                        entry.url.replace('>', "%3E"),
                        entry.description
                    )
                } else {
                    entry.description.clone()
                };
                ensure!(!description.trim().is_empty(), "Descrição obrigatória");
                limit(&title, 256)?;
                limit(&description, 1024)?;
                total += title.encode_utf16().count() + description.encode_utf16().count();
                embed = embed.field(title, description, false);
            }
            if index + 1 == count && !self.footer.is_empty() {
                total += self.footer.encode_utf16().count();
                embed = embed.footer(s::CreateEmbedFooter::new(&self.footer));
            }
            embeds.push(embed);
        }
        ensure!(
            total <= 6000,
            "Embeds excedem 6000 caracteres; reduza o conteúdo antes de publicar"
        );
        Ok(embeds)
    }

    fn rules_embed(&self, entries: &[Entry]) -> Result<Vec<s::CreateEmbed>> {
        // Normalize only section boundaries; preserve internal TOML line breaks verbatim.
        let boundary = |c| c == '\n' || c == '\r';
        let mut sections = Vec::new();
        let introduction = self.description.trim_matches(boundary);
        if !introduction.is_empty() {
            sections.push(introduction.to_owned());
        }
        for (index, entry) in entries.iter().enumerate() {
            let body = entry.description.trim_matches(boundary);
            ensure!(!body.trim().is_empty(), "Descrição obrigatória");
            let number = index + 1;
            let prefix = ["1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"]
                .get(index)
                .map_or_else(|| format!("{number}."), |value| value.to_string());
            let title = entry.title.trim_matches(boundary);
            let heading = if entry.emoji.is_empty() {
                format!("{prefix} {title}")
            } else {
                format!("{prefix} {} {title}", entry.emoji)
            };
            sections.push(format!("**{heading}**\n{body}"));
        }
        let ending = self.footer.trim_matches(boundary);
        if !ending.is_empty() {
            sections.push(ending.to_owned());
        }
        let description = sections.join("\n\n");
        limit(&description, 4096)?;
        ensure!(
            self.title.encode_utf16().count() + description.encode_utf16().count() <= 6000,
            "Embed de regras excede 6000 caracteres; reduza o conteúdo antes de publicar"
        );
        ensure!(
            !self.title.is_empty() || !description.is_empty(),
            "Embed de regras vazio"
        );
        let mut embed = s::CreateEmbed::new()
            .color(self.color)
            .title(&self.title)
            .description(description);
        if !self.thumbnail.is_empty() {
            embed = embed.thumbnail(&self.thumbnail);
        }
        if !self.image.is_empty() {
            embed = embed.image(&self.image);
        }
        Ok(vec![embed])
    }

    pub fn discord_invite(&self) -> Result<Option<&Entry>> {
        let mut invite = None;
        for entry in &self.entries {
            entry.validate_link()?;
            if entry.is_discord_invite()? {
                ensure!(
                    invite.is_none(),
                    "Quadro possui mais de um convite principal do Discord; mantenha somente um convite por quadro"
                );
                invite = Some(entry);
            }
        }
        Ok(invite)
    }

    fn links_embed(&self, entries: &[Entry]) -> Result<Vec<s::CreateEmbed>> {
        let invite = self.discord_invite()?;
        let mut sections = Vec::new();
        let intro = self.description.trim_matches(['\r', '\n']);
        if !intro.is_empty() {
            sections.push(intro.to_owned());
        }
        for entry in entries
            .iter()
            .filter(|e| invite.is_none_or(|i| i.id != e.id))
            .chain(invite)
        {
            let title = if entry.emoji.is_empty() {
                entry.title.clone()
            } else {
                format!("{} {}", entry.emoji, entry.title)
            };
            let action = if entry.is_discord_invite()? {
                "Copiar convite ↓".into()
            } else {
                let label = entry
                    .label
                    .as_deref()
                    .unwrap_or("Abrir link")
                    .replace('\\', "\\\\")
                    .replace('[', "\\[")
                    .replace(']', "\\]");
                format!(
                    "[{label}](<{}>)",
                    entry.url.replace('>', "%3E").replace('<', "%3C")
                )
            };
            let mut block = format!("**{title} — {action}**");
            let description = entry.description.trim_matches(['\r', '\n']);
            if !description.is_empty() {
                block.push('\n');
                block.push_str(description);
            }
            sections.push(block);
        }
        let description = sections.join("\n\n");
        limit(&description, 4096)?;
        ensure!(
            self.title.encode_utf16().count()
                + description.encode_utf16().count()
                + self.footer.encode_utf16().count()
                <= 6000,
            "Embed de links excede 6000 caracteres; nada foi truncado"
        );
        ensure!(
            !self.title.is_empty() || !description.is_empty(),
            "Quadro de Links vazio"
        );
        let mut embed = s::CreateEmbed::new()
            .title(&self.title)
            .description(description)
            .color(self.color);
        if !self.thumbnail.is_empty() {
            embed = embed.thumbnail(&self.thumbnail);
        }
        if !self.image.is_empty() {
            embed = embed.image(&self.image);
        }
        if !self.footer.is_empty() {
            embed = embed.footer(s::CreateEmbedFooter::new(&self.footer));
        }
        Ok(vec![embed])
    }
}
