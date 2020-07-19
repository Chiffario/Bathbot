use crate::{util::MessageExt, BotResult, Context};

use std::{sync::Arc, time::Instant};
use twilight::model::channel::Message;

#[command]
#[short_desc("Check if I'm online")]
#[long_desc(
    "Check if I'm online.\n\
    The latency indicates how fast I receive messages from Discord."
)]
#[aliases("p")]
async fn ping(ctx: Arc<Context>, msg: &Message) -> BotResult<()> {
    let start = Instant::now();
    let response = ctx
        .http
        .create_message(msg.channel_id)
        .content(":ping_pong:")?
        .await?;
    let elapsed = (Instant::now() - start).as_millis();
    ctx.http
        .update_message(msg.channel_id, response.id)
        .content(Some(format!(":ping_pong: Pong! ({}ms)", elapsed)))?
        .await?;
    response.reaction_delete(&ctx, msg.author.id);
    Ok(())
}
