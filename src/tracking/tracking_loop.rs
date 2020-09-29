use crate::{
    embeds::{EmbedData, TrackNotificationEmbed},
    Context,
};

use futures::future::{join_all, FutureExt};
use rosu::{
    backend::{BestRequest, UserRequest},
    models::{Beatmap, GameMode, Score, User},
};
use std::{collections::HashMap, sync::Arc};
use tokio::time;

pub async fn tracking_loop(ctx: Arc<Context>) {
    let delay = time::Duration::from_secs(60);
    loop {
        // Get all users that should be tracked in this iteration
        let tracked = match ctx.tracking().pop().await {
            Some(tracked) => tracked,
            None => {
                time::delay_for(delay).await;
                continue;
            }
        };
        // Build top score requests for each
        let score_futs = tracked.keys().map(|(user_id, mode)| {
            BestRequest::with_user_id(*user_id)
                .mode(*mode)
                .limit(100)
                .queue(ctx.osu())
                .map(move |result| (*user_id, *mode, result))
        });
        // Iterate over the request responses
        let mut maps: HashMap<u32, Beatmap> = HashMap::new();
        for (user_id, mode, result) in join_all(score_futs).await {
            match result {
                Ok(scores) => process_tracking(&ctx, mode, &scores, None, &mut maps).await,
                Err(why) => {
                    warn!(
                        "API issue while retrieving user ({},{}) for tracking: {}",
                        user_id, mode, why
                    );
                    ctx.tracking().reset(user_id, mode).await;
                }
            }
        }
    }
}

pub async fn process_tracking(
    ctx: &Context,
    mode: GameMode,
    scores: &[Score],
    user: Option<&User>,
    maps: &mut HashMap<u32, Beatmap>,
) {
    let id_option = scores
        .first()
        .map(|s| s.user_id)
        .or_else(|| user.map(|u| u.user_id));
    let user_id = match id_option {
        Some(id) => id,
        None => return,
    };
    let (last, channels) = match ctx.tracking().get_tracked(user_id, mode) {
        Some(tuple) => tuple,
        None => return,
    };
    let max = match channels.values().max() {
        Some(max) => *max,
        None => {
            warn!("No tracked channels for ({},{})", user_id, mode);
            return;
        }
    };
    let new_last = scores.iter().map(|s| s.date).max();
    debug!(
        "[Tracking] ({},{}): last {} | curr {}",
        user_id,
        mode,
        last,
        new_last.unwrap()
    );
    let mut user_value = None; // will be set if user is None but there is new top score
    for (idx, score) in scores.iter().enumerate().take(max) {
        // Skip if its an older score
        if score.date <= last {
            continue;
        }
        debug!(
            "[New top score] ({},{}): new {} | old {}",
            user_id, mode, score.date, last
        );
        // Prepare beatmap
        let map_id = match score.beatmap_id {
            Some(id) => id,
            None => {
                warn!("No beatmap_id for ({},{})'s score", user_id, mode);
                continue;
            }
        };
        if !maps.contains_key(&map_id) {
            match ctx.psql().get_beatmap(map_id).await {
                Ok(map) => maps.insert(map_id, map),
                Err(_) => match score.get_beatmap(ctx.osu()).await {
                    Ok(map) => maps.insert(map_id, map),
                    Err(why) => {
                        warn!("Error while retrieving tracking map id {}: {}", map_id, why);
                        continue;
                    }
                },
            };
        }
        let map = maps.get(&map_id).unwrap();
        // Prepare user
        let user = match (user, user_value.as_ref()) {
            (Some(user), _) => user,
            (None, Some(user)) => user,
            (None, None) => {
                let user_fut = UserRequest::with_user_id(user_id)
                    .mode(mode)
                    .queue_single(ctx.osu());
                match user_fut.await {
                    Ok(Some(user)) => {
                        user_value = Some(user);
                        user_value.as_ref().unwrap()
                    }
                    Ok(None) => {
                        warn!("Empty result while retrieving tracking user {}", user_id);
                        continue;
                    }
                    Err(why) => {
                        warn!("Error while retrieving tracking user {}: {}", user_id, why);
                        continue;
                    }
                }
            }
        };
        // Build embed
        let data = TrackNotificationEmbed::new(ctx, user, score, map, idx + 1).await;
        let embed = match data.build().build() {
            Ok(embed) => embed,
            Err(why) => {
                warn!("Error while creating tracking notification embed: {}", why);
                continue;
            }
        };
        // Send the embed to each tracking channel
        for (&channel, &limit) in channels.iter() {
            if idx + 1 > limit {
                continue;
            }
            // Try to build and send the message
            match ctx.http.create_message(channel).embed(embed.clone()) {
                Ok(msg_fut) => {
                    if let Err(why) = msg_fut.await {
                        warn!(
                            "Error while sending osu!tracking notif ({}): {}",
                            channel, why
                        );
                    }
                }
                Err(why) => warn!("Invalid embed for osu!tracking notification: {}", why),
            }
        }
    }
    if let Some(new_date) = new_last.filter(|&d| d > last) {
        debug!(
            "[Tracking] Updating for ({},{}): {} -> {}",
            user_id,
            mode,
            last,
            new_last.unwrap()
        );
        let update_fut = ctx
            .tracking()
            .update_last_date(user_id, mode, new_date, ctx.psql());
        if let Err(why) = update_fut.await {
            warn!(
                "Error while updating tracking date for user ({},{}): {}",
                user_id, mode, why
            );
        }
    }
    ctx.tracking().reset(user_id, mode).await;
    debug!("[Tracking] Reset ({},{})", user_id, mode);
}