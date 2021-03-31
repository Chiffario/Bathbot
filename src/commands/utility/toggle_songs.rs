use crate::{util::MessageExt, Args, BotResult, Context};

use std::sync::Arc;
use twilight_model::channel::Message;

#[command]
#[only_guilds()]
#[authority()]
#[short_desc("Toggle availability of song commands in a server")]
#[long_desc(
    "Toggle whether song commands can be used in this server. \
    Defaults to `true`"
)]
#[aliases("songstoggle", "songtoggle")]
async fn togglesongs(ctx: Arc<Context>, msg: &Message, _: Args) -> BotResult<()> {
    let guild_id = msg.guild_id.unwrap();
    let mut with_lyrics = false;

    ctx.update_config(guild_id, |config| {
        config.with_lyrics = !config.with_lyrics;
        with_lyrics = config.with_lyrics;
    });

    let content = if with_lyrics {
        "Song commands can now be used in this server"
    } else {
        "Song commands can no longer be used in this server"
    };

    msg.send_response(&ctx, content).await?;

    Ok(())
}