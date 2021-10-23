use crate::{
    custom_client::OsuStatsPlayer,
    embeds::{Author, Footer},
    util::{
        constants::{AVATAR_URL, OSU_BASE},
        numbers::with_comma_int,
        osu::flag_url,
        CountryCode,
    },
};

use std::fmt::Write;

pub struct OsuStatsListEmbed {
    author: Author,
    description: String,
    footer: Footer,
    thumbnail: String,
}

impl OsuStatsListEmbed {
    pub fn new(
        players: &[OsuStatsPlayer],
        country: &Option<CountryCode>,
        first_place_id: u32,
        pages: (usize, usize),
    ) -> Self {
        let mut author = Author::new("Most global leaderboard scores");

        if let Some(country) = country {
            author = author.icon_url(flag_url(country.as_str()));
        }

        let mut description = String::with_capacity(1024);

        for (i, player) in players.iter().enumerate() {
            let _ = writeln!(
                description,
                "**{}. [{}]({}users/{})**: {}",
                (pages.0 - 1) * 15 + i + 1,
                player.username,
                OSU_BASE,
                player.user_id,
                with_comma_int(player.count)
            );
        }

        Self {
            author,
            description,
            footer: Footer::new(format!("Page {}/{}", pages.0, pages.1)),
            thumbnail: format!("{}{}", AVATAR_URL, first_place_id),
        }
    }
}

impl_builder!(OsuStatsListEmbed {
    author,
    description,
    footer,
    thumbnail,
});
