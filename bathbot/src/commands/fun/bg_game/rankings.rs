use std::collections::{BTreeMap, HashSet};

use bathbot_model::{RankingEntries, RankingEntry, RankingKind};
use bathbot_util::{IntHasher, constants::GENERAL_ISSUE};
use eyre::Result;
use twilight_model::{channel::Message, id::Id};

use crate::{
    Context,
    active::{ActiveMessages, impls::RankingPagination},
    util::ChannelExt,
};

pub async fn leaderboard(msg: &Message, global: bool) -> Result<()> {
    let cache = Context::cache();

    let mut scores = match Context::games().bggame_leaderboard().await {
        Ok(scores) => scores,
        Err(err) => {
            let _ = msg.error(GENERAL_ISSUE).await;

            return Err(err.wrap_err("failed to get bggame scores"));
        }
    };

    let guild = msg.guild_id;

    if let Some(guild) = guild.filter(|_| !global) {
        let members: HashSet<_, IntHasher> = cache
            .members(guild)
            .await?
            .into_iter()
            .map(|id| id as i64)
            .collect();

        scores.retain(|row| members.contains(&row.discord_id));
    }

    let author = msg.author.id.get() as i64;

    scores.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    let author_idx = scores.iter().position(|row| row.discord_id == author);

    // Gather usernames for initial page
    let mut entries = BTreeMap::new();

    for (i, row) in scores.iter().enumerate().take(20) {
        let id = Id::new(row.discord_id as u64);

        let name_opt = match Context::user_config().osu_name(id).await {
            Ok(Some(name)) => Some(name),
            Ok(None) => match cache.user(id).await {
                Ok(Some(user)) => Some(user.name.as_ref().into()),
                Ok(None) => None,
                Err(err) => {
                    warn!("{err:?}");

                    None
                }
            },
            Err(err) => {
                warn!("{err:?}");

                None
            }
        };

        let name = name_opt.unwrap_or_else(|| "<unknown user>".into());

        let entry = RankingEntry {
            value: row.score as u64,
            name,
            country: None,
        };

        entries.insert(i, entry);
    }

    let entries = RankingEntries::Amount(entries);

    // Prepare initial page
    let total = scores.len();
    let global = guild.is_none() || global;
    let data = RankingKind::BgScores { global, scores };

    let pagination = RankingPagination::builder()
        .entries(entries)
        .total(total)
        .author_idx(author_idx)
        .kind(data)
        .defer(false)
        .msg_owner(msg.author.id)
        .build();

    ActiveMessages::builder(pagination).begin(msg).await
}
