use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult, Write},
};

use bathbot_macros::EmbedData;
use bathbot_model::{MEDAL_GROUPS, MedalGroup, OsekaiMedal};
use bathbot_util::{
    AuthorBuilder, FooterBuilder, IntHasher, attachment, fields, numbers::round, osu::flag_url,
};
use rosu_v2::prelude::MedalCompact;
use twilight_model::channel::message::embed::EmbedField;

use crate::manager::redis::osu::CachedUser;

#[derive(EmbedData)]
pub struct MedalStatsEmbed {
    author: AuthorBuilder,
    fields: Vec<EmbedField>,
    footer: FooterBuilder,
    image: String,
}

impl MedalStatsEmbed {
    pub fn new(
        user: &CachedUser,
        user_medals: &[MedalCompact],
        medals: &HashMap<u32, StatsMedal, IntHasher>,
        rarest: Option<MedalCompact>,
        with_graph: bool,
    ) -> Self {
        let completion = round(100.0 * user_medals.len() as f32 / medals.len() as f32);

        let mut fields = fields![
            "Medals", format!("{} / {}", user_medals.len(), medals.len()), true;
            "Completion", format!("{completion}%"), true;
        ];

        let oldest = user_medals.first();
        let newest = user_medals.last();

        if oldest.or(newest).or(rarest.as_ref()).is_some() {
            let mut value = String::with_capacity(128);

            if let Some((StatsMedal { name, rarity, .. }, date)) =
                oldest.and_then(|medal| Some((medals.get(&medal.medal_id)?, medal.achieved_at)))
            {
                let _ = writeln!(
                    value,
                    "👴 `Oldest` [{name}]({url}) <t:{timestamp}:d>",
                    url = MedalUrl { name, rarity },
                    timestamp = date.unix_timestamp()
                );
            }

            if let Some((StatsMedal { name, rarity, .. }, date)) =
                newest.and_then(|medal| Some((medals.get(&medal.medal_id)?, medal.achieved_at)))
            {
                let _ = writeln!(
                    value,
                    "👶 `Newest` [{name}]({url}) <t:{timestamp}:d>",
                    url = MedalUrl { name, rarity },
                    timestamp = date.unix_timestamp()
                );
            }

            if let Some((StatsMedal { name, rarity, .. }, date)) =
                rarest.and_then(|medal| Some((medals.get(&medal.medal_id)?, medal.achieved_at)))
            {
                let _ = writeln!(
                    value,
                    "💎 `Rarest` [{name}]({url}) <t:{timestamp}:d>",
                    url = MedalUrl { name, rarity },
                    timestamp = date.unix_timestamp()
                );
            }

            fields![fields { "Cornerstone medals", value, false }];
        }

        if !user_medals.is_empty() {
            let mut counts = HashMap::new();

            // Count groups for all medals
            for medal in medals.values() {
                let (total, _) = counts.entry(medal.group.as_str()).or_insert((0, 0));
                *total += 1;
            }

            // Count groups for owned medals
            for medal in user_medals.iter() {
                let entry = medals
                    .get(&medal.medal_id)
                    .and_then(|medal| counts.get_mut(medal.group.as_str()));

                if let Some((_, owned)) = entry {
                    *owned += 1;
                }
            }

            // Adjust the order a little to improve formatting
            let mut groups = MEDAL_GROUPS;
            groups.swap(0, 1);
            groups.swap(1, 2);

            // Add to fields
            groups.iter().map(|group| group.as_str()).for_each(|group| {
                if let Some((total, owned)) = counts.get(group) {
                    let value = format!("{owned} / {total}");
                    fields![fields { group.to_string(), value, true }];
                }
            });
        }

        let country_code = user.country_code.as_str();
        let username = user.username.as_str();
        let user_id = user.user_id.to_native();

        let author = AuthorBuilder::new(username)
            .url(format!(
                "https://osekai.net/profiles/?user={user_id}&mode=all"
            ))
            .icon_url(flag_url(country_code));

        let footer = FooterBuilder::new("Check osekai.net for more info");

        let image = if with_graph {
            attachment("medal_graph.png")
        } else {
            String::new()
        };

        Self {
            image,
            author,
            fields,
            footer,
        }
    }
}

struct MedalUrl<'n> {
    name: &'n str,
    rarity: &'n f32,
}

impl Display for MedalUrl<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let url = match OsekaiMedal::name_to_url(self.name) {
            Ok(url) => url,
            Err(err) => {
                warn!(?err);

                OsekaiMedal::backup_name_to_url(self.name)
            }
        };

        write!(f, "{url} \"Rarity: {rarity}%\"", rarity = self.rarity)
    }
}

pub struct StatsMedal {
    pub name: Box<str>,
    pub group: MedalGroup,
    pub rarity: f32,
}
