use crate::{
    commands::osu::RankData,
    embeds::Author,
    util::{
        numbers::{with_comma_float, with_comma_int},
        osu::pp_missing,
    },
};

use rosu_v2::model::score::Score;

pub struct RankEmbed {
    description: String,
    title: String,
    thumbnail: String,
    author: Author,
}

impl RankEmbed {
    pub fn new(data: RankData, scores: Option<Vec<Score>>) -> Self {
        let (title, description) = match &data {
            RankData::Sub10k {
                user,
                rank,
                country,
                rank_holder,
            } => {
                let user_pp = user.statistics.as_ref().unwrap().pp;
                let rank_holder_pp = rank_holder.statistics.as_ref().unwrap().pp;

                let country = country.as_ref().map(|code| code.as_str()).unwrap_or("#");

                let title = format!(
                    "How many pp is {name} missing to reach rank {country}{rank}?",
                    name = user.username,
                    country = country,
                    rank = rank
                );

                let description = if user.user_id == rank_holder.user_id {
                    format!("{} is already at rank #{}.", user.username, rank)
                } else if user_pp > rank_holder_pp {
                    format!(
                        "Rank {country}{rank} is currently held by {holder_name} with \
                        **{holder_pp}pp**, so {name} is already above that with **{pp}pp**.",
                        country = country,
                        rank = rank,
                        holder_name = rank_holder.username,
                        holder_pp = with_comma_float(rank_holder_pp),
                        name = user.username,
                        pp = with_comma_float(user_pp)
                    )
                } else if let Some(scores) = scores {
                    let (required, _) = pp_missing(user_pp, rank_holder_pp, &scores);

                    format!(
                        "Rank {country}{rank} is currently held by {holder_name} with \
                        **{holder_pp}pp**, so {name} is missing **{missing}** raw pp, \
                        achievable with a single score worth **{pp}pp**.",
                        country = country,
                        rank = rank,
                        holder_name = rank_holder.username,
                        holder_pp = with_comma_float(rank_holder_pp),
                        name = user.username,
                        missing = with_comma_float(rank_holder_pp - user_pp),
                        pp = with_comma_float(required),
                    )
                } else {
                    format!(
                        "Rank {country}{rank} is currently held by {holder_name} with \
                        **{holder_pp}pp**, so {name} is missing **{holder_pp}** raw pp, \
                        achievable with a single score worth **{holder_pp}pp**.",
                        country = country,
                        rank = rank,
                        holder_name = rank_holder.username,
                        holder_pp = with_comma_float(rank_holder_pp),
                        name = user.username,
                    )
                };

                (title, description)
            }
            RankData::Over10k {
                user,
                rank,
                required_pp,
            } => {
                let user_pp = user.statistics.as_ref().unwrap().pp;

                let title = format!(
                    "How many pp is {name} missing to reach rank #{rank}?",
                    name = user.username,
                    rank = with_comma_int(*rank),
                );

                let description = if user_pp > *required_pp {
                    format!(
                        "Rank #{rank} currently requires **{required_pp}pp**, \
                        so {name} is already above that with **{pp}pp**.",
                        rank = with_comma_int(*rank),
                        required_pp = with_comma_float(*required_pp),
                        name = user.username,
                        pp = with_comma_float(user_pp)
                    )
                } else if let Some(scores) = scores {
                    let (required, _) = pp_missing(user_pp, *required_pp, &scores);

                    format!(
                        "Rank #{rank} currently requires **{required_pp}pp**, \
                        so {name} is missing **{missing}** raw pp, \
                        achievable with a single score worth **{pp}pp**.",
                        rank = with_comma_int(*rank),
                        required_pp = with_comma_float(*required_pp),
                        name = user.username,
                        missing = with_comma_float(required_pp - user_pp),
                        pp = with_comma_float(required),
                    )
                } else {
                    format!(
                        "Rank #{rank} currently requires **{required_pp}pp**, \
                        so {name} is missing **{required_pp}** raw pp, \
                        achievable with a single score worth **{required_pp}pp**.",
                        rank = with_comma_int(*rank),
                        required_pp = with_comma_float(*required_pp),
                        name = user.username,
                    )
                };

                (title, description)
            }
        };

        let user = data.user();

        Self {
            title,
            description,
            author: author!(user),
            thumbnail: user.avatar_url,
        }
    }
}

impl_builder!(RankEmbed {
    author,
    description,
    thumbnail,
    title,
});
