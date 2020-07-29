use super::require_link;
use crate::{
    arguments::{Args, SimulateNameArgs},
    embeds::{EmbedData, SimulateEmbed},
    util::{
        constants::{GENERAL_ISSUE, OSU_API_ISSUE},
        MessageExt,
    },
    BotResult, Context,
};

use rosu::{
    backend::requests::RecentRequest,
    models::{
        ApprovalStatus::{Approved, Loved, Ranked},
        GameMode,
    },
};
use std::sync::Arc;
use tokio::time::{self, Duration};
use twilight::model::channel::Message;

#[allow(clippy::cognitive_complexity)]
async fn simulate_recent_main(
    mode: GameMode,
    ctx: Arc<Context>,
    msg: &Message,
    args: Args<'_>,
) -> BotResult<()> {
    let mut args = match SimulateNameArgs::new(args) {
        Ok(args) => args,
        Err(err_msg) => return msg.respond(&ctx, err_msg).await,
    };
    let name = match args.name.take().or_else(|| ctx.get_link(msg.author.id.0)) {
        Some(name) => name,
        None => return require_link(&ctx, msg).await,
    };

    // Retrieve the recent score
    let request = RecentRequest::with_username(&name).mode(mode).limit(1);
    let score = match request.queue(ctx.osu()).await {
        Ok(mut scores) => match scores.pop() {
            Some(score) => score,
            None => {
                let content = format!("No recent plays found for user `{}`", name);
                return msg.respond(&ctx, content).await;
            }
        },
        Err(why) => {
            msg.respond(&ctx, OSU_API_ISSUE).await?;
            return Err(why.into());
        }
    };

    // Retrieving the score's beatmap
    let map = match ctx.psql().get_beatmap(score.beatmap_id.unwrap()).await {
        Ok(map) => map,
        Err(_) => match score.get_beatmap(ctx.osu()).await {
            Ok(m) => m,
            Err(why) => {
                msg.respond(&ctx, OSU_API_ISSUE).await?;
                return Err(why.into());
            }
        },
    };

    // Accumulate all necessary data
    let data = match SimulateEmbed::new(&ctx, Some(score), &map, args.into()).await {
        Ok(data) => data,
        Err(why) => {
            msg.respond(&ctx, GENERAL_ISSUE).await?;
            return Err(why);
        }
    };

    // Creating the embed
    let embed = data.build().build();
    let response = ctx
        .http
        .create_message(msg.channel_id)
        .content("Simulated score:")?
        .embed(embed)?
        .await?;

    // Add map to database if its not in already
    if let Err(why) = ctx.psql().insert_beatmap(&map).await {
        warn!("Could not add map to DB: {}", why);
    }

    response.reaction_delete(&ctx, msg.author.id);

    // Minimize embed after delay
    time::delay_for(Duration::from_secs(45)).await;
    let embed = data.minimize().build();
    let edit_fut = ctx
        .http
        .update_message(response.channel_id, response.id)
        .embed(embed)?;
    if let Err(why) = edit_fut.await {
        warn!("Error while minimizing simulate recent msg: {}", why);
    }
    Ok(())
}

#[command]
#[short_desc("Display an unchoked version of user's most recent play")]
#[usage("[username] [+mods] [-a acc%] [-300 #300s] [-100 #100s] [-50 #50s] [-m #misses]")]
#[example("badewanne3 +hr -a 99.3 -300 1422 -m 1")]
#[aliases("sr")]
pub async fn simulaterecent(ctx: Arc<Context>, msg: &Message, args: Args) -> BotResult<()> {
    simulate_recent_main(GameMode::STD, ctx, msg, args).await
}

#[command]
#[short_desc("Display a perfect play on a user's most recently played mania map")]
#[usage("[username] [+mods] [-s score]")]
#[example("badewanne3 +dt -s 8950000")]
#[aliases("srm")]
pub async fn simulaterecentmania(ctx: Arc<Context>, msg: &Message, args: Args) -> BotResult<()> {
    simulate_recent_main(GameMode::MNA, ctx, msg, args).await
}