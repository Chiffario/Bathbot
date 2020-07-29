use crate::{util::MessageExt, Args, BotResult, Context};

use std::sync::Arc;
use twilight::model::channel::Message;

#[command]
#[short_desc("Stop the bg game")]
#[aliases("end", "quit")]
pub async fn stop(ctx: Arc<Context>, msg: &Message, _: Args) -> BotResult<()> {
    if let Err(why) = ctx.stop_and_remove_game(msg.channel_id).await {
        msg.respond(&ctx, "Error while stopping game \\:(").await?;
        return Err(why);
    }
    Ok(())
}