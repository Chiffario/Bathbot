use crate::{util::MessageExt, Args, BotResult, Context};

use std::sync::{atomic::Ordering, Arc};
use twilight_model::channel::Message;

#[command]
#[short_desc("Toggle osu!tracking")]
#[owner()]
async fn trackingtoggle(ctx: Arc<Context>, msg: &Message, _: Args) -> BotResult<()> {
    ctx.tracking()
        .stop_tracking
        .fetch_nand(true, Ordering::SeqCst);

    let current = ctx.tracking().stop_tracking.load(Ordering::Acquire);
    let content = format!("Tracking toggle: {} -> {}", current, !current);
    msg.send_response(&ctx, content).await?;

    Ok(())
}
