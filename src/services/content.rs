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
        limit(&self.footer, 2048)?;
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
        let fields_per_embed = if kind == "rules" {
            ensure!(
                entries.len() <= 25,
                "Regras excedem 25 campos; reduza a quantidade para publicar em um único embed"
            );
            25
        } else {
            5
        };
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
}
