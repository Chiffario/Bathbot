use std::{fmt::Write, time::Duration};

use command_macros::SlashCommand;
use dashmap::mapref::entry::Entry;
use eyre::Report;
use image::{png::PngEncoder, ColorType, GenericImageView, ImageBuffer};
use rand::Rng;
use rosu_v2::prelude::{CountryCode, GameMode, GameMods, Grade, Username};
use tokio::time::sleep;
use twilight_interactions::command::CreateCommand;
use twilight_model::{
    application::{
        component::{button::ButtonStyle, ActionRow, Button, Component},
        interaction::{ApplicationCommand, MessageComponentInteraction},
    },
    channel::embed::{Embed, EmbedField},
    id::{
        marker::{ChannelMarker, GuildMarker, MessageMarker, UserMarker},
        Id,
    },
};

use crate::{
    embeds::get_mods,
    util::{
        builder::{EmbedBuilder, MessageBuilder},
        constants::{GENERAL_ISSUE, HL_IMAGE_CHANNEL_ID, RED},
        numbers::{round, with_comma_int},
        osu::grade_emote,
        ApplicationCommandExt, Authored, ChannelExt, ComponentExt, Emote,
    },
    BotResult, Context,
};

use std::{borrow::Cow, mem, sync::Arc};

const W: u32 = 900;
const H: u32 = 250;
const ALPHA_THRESHOLD: u8 = 20;

#[derive(CreateCommand, SlashCommand)]
#[command(
    name = "higherlower",
    help = "Play a game of osu! themed higher lower.\n\
    The available modes are:\n \
    - `PP`: Guess whether the next play is worth higher or lower PP!"
)]
/// Play a game of osu! themed higher lower
pub struct HigherLower;

#[derive(CreateCommand, SlashCommand)]
#[command(
    name = "higherlower",
    help = "Play a game of osu! themed higher lower.\n\
    The available modes are:\n \
    - `PP`: Guess whether the next play is worth higher or lower PP!"
)]
/// Play a game of osu! themed higher lower
pub struct Hl;

async fn slash_higherlower(ctx: Arc<Context>, command: Box<ApplicationCommand>) -> BotResult<()> {
    // TODO: handle modes, add different modes, add difficulties and difficulty increase
    let user = command.user_id()?;
    let content = ctx.hl_games().get(&user).map(|v| {
        let game = v.value();
        format!(
            "You can't play two higher lower games at once! \n\
            Finish your [other game](https://discord.com/channels/{}/{}/{}) first or give up.",
            match game.guild {
                Some(id) => id.to_string(),
                None => "@me".to_string(),
            },
            game.channel,
            game.id
        )
    });

    if let Some(content) = content {
        let components = give_up_components();
        let embed = EmbedBuilder::new().color(RED).description(content).build();

        let builder = MessageBuilder::new().embed(embed).components(components);
        command.update(&ctx, &builder).await?;
    } else {
        let (play1, mut play2) =
            match tokio::try_join!(random_play(&ctx, 0.0, 0), random_play(&ctx, 0.0, 0)) {
                Ok(tuple) => tuple,
                Err(err) => {
                    let _ = command.error(&ctx, GENERAL_ISSUE).await;
                    return Err(err);
                }
            };
        while play2 == play1 {
            play2 = random_play(&ctx, 0.0, 0).await?;
        }

        //TODO: handle mode
        let mut game = HlGameState {
            previous: play1,
            next: play2,
            player: user,
            id: Id::new(1),
            channel: command.channel_id(),
            guild: command.guild_id(),
            mode: 1,
            current_score: 0,
            highscore: ctx.psql().get_higherlower_highscore(user.get(), 1).await?,
        };

        let image = game.create_image(&ctx).await?;
        let components = hl_components();
        let embed = game.to_embed(image);

        let builder = MessageBuilder::new().embed(embed).components(components);
        let response = command.update(&ctx, &builder).await?.model().await?;

        game.id = response.id;
        ctx.hl_games().insert(user, game);
    }

    Ok(())
}

async fn slash_hl(ctx: Arc<Context>, command: Box<ApplicationCommand>) -> BotResult<()> {
    slash_higherlower(ctx, command).await
}

async fn random_play(ctx: &Context, prev_pp: f32, curr_score: u32) -> BotResult<HlGameStateInfo> {
    let max_play = 1.max(25 - curr_score);
    let (rank, play): (u32, u32) = {
        let mut rng = rand::thread_rng();
        (rng.gen_range(1..=5000), rng.gen_range(0..max_play))
    };

    let page = ((rank - 1) / 50) + 1;
    let idx = (rank - 1) % 50;

    // ! Currently 3 requests, can probably be reduced
    let player = ctx
        .osu()
        .performance_rankings(GameMode::STD)
        .page(page)
        .await?
        .ranking
        .swap_remove(idx as usize);

    let mut plays = ctx
        .osu()
        .user_scores(player.user_id)
        .limit(50)
        // .offset(play as usize)
        .mode(GameMode::STD)
        .best()
        .await?;

    plays.sort_by(|a, b| {
        (round(a.pp.unwrap_or(0.0)) - prev_pp)
            .abs()
            .partial_cmp(&(round(b.pp.unwrap_or(0.0)) - prev_pp).abs())
            .unwrap()
    });

    let play = plays.swap_remove(play as usize);

    let map_id = play.map.as_ref().unwrap().map_id;

    let map = match ctx.psql().get_beatmap(map_id, true).await {
        Ok(map) => map,
        Err(_) => match ctx.osu().beatmap().map_id(map_id).await {
            Ok(map) => {
                // Store map in DB
                if let Err(err) = ctx.psql().insert_beatmap(&map).await {
                    warn!("{:?}", Report::new(err));
                }

                map
            }
            Err(err) => {
                return Err(err.into());
            }
        },
    };

    let rank = player
        .statistics
        .as_ref()
        .and_then(|stats| stats.global_rank)
        .unwrap_or(0);

    let mapset = map.mapset.unwrap();

    Ok(HlGameStateInfo {
        user_id: player.user_id,
        username: player.username,
        avatar: player.avatar_url,
        rank,
        country_code: player.country_code,
        map_id: map.map_id,
        map_string: format!(
            "[{} - {} [{}]]({})",
            mapset.artist, mapset.title, map.version, map.url
        ),
        mods: play.mods,
        pp: round(play.pp.unwrap_or(0.0)),
        combo: play.max_combo,
        max_combo: map.max_combo.unwrap_or(0),
        score: play.score,
        acc: round(play.accuracy),
        miss_count: play.statistics.count_miss,
        grade: play.grade,
        cover: format!(
            "https://assets.ppy.sh/beatmaps/{}/covers/cover.jpg",
            mapset.mapset_id
        ),
    })
}

pub struct HlGameState {
    previous: HlGameStateInfo,
    next: HlGameStateInfo,
    #[allow(unused)] // TODO
    player: Id<UserMarker>,
    id: Id<MessageMarker>,
    channel: Id<ChannelMarker>,
    guild: Option<Id<GuildMarker>>,
    mode: u8,
    current_score: u32,
    highscore: u32,
}

impl HlGameState {
    fn to_embed(&self, image: String) -> Embed {
        let title = "Higher or Lower: PP";
        let mut fields = Vec::new();
        fields.push(EmbedField {
            inline: false,
            name: format!("__Previous:__ {}", self.previous.player_string()),
            value: self.previous.play_string(true),
        });
        fields.push(EmbedField {
            inline: false,
            name: format!("__Next:__ {}", self.next.player_string()),
            value: self.next.play_string(false),
        });
        let footer = format!(
            "Current score: {} • Highscore: {}",
            self.current_score, self.highscore
        );
        let embed = EmbedBuilder::new()
            .title(title)
            .fields(fields)
            .image(image)
            .footer(footer)
            .build();

        embed
    }

    fn check_guess(&self, guess: HlGuess) -> bool {
        match guess {
            HlGuess::Higher => self.next.pp >= self.previous.pp,
            HlGuess::Lower => self.next.pp <= self.previous.pp,
        }
    }

    async fn create_image(&self, ctx: &Context) -> BotResult<String> {
        let pfp_left =
            image::load_from_memory(&ctx.client().get_avatar(&self.previous.avatar).await?)?
                .thumbnail(128, 128);
        let pfp_right =
            image::load_from_memory(&ctx.client().get_avatar(&self.next.avatar).await?)?
                .thumbnail(128, 128);

        let bg_left =
            image::load_from_memory(&ctx.client().get_mapset_cover(&self.previous.cover).await?)?;

        let bg_right =
            image::load_from_memory(&ctx.client().get_mapset_cover(&self.next.cover).await?)?;

        let mut blipped = ImageBuffer::new(W, H);

        let iter = blipped
            .enumerate_pixels_mut()
            .zip(bg_left.pixels())
            .zip(bg_right.pixels());

        for (((x, _, pixel), (.., left)), (.., right)) in iter {
            *pixel = if x <= W / 2 { left } else { right };
        }

        for (x, y, pixel) in pfp_left.pixels() {
            if pixel.0[3] > ALPHA_THRESHOLD {
                blipped.put_pixel(x, y, pixel);
            }
        }

        let pfp_right_width = pfp_right.width();

        for (x, y, pixel) in pfp_right.pixels() {
            if pixel.0[3] > ALPHA_THRESHOLD {
                blipped.put_pixel(W - pfp_right_width + x, y, pixel);
            }
        }

        let mut png_bytes: Vec<u8> = Vec::with_capacity((W * H * 4) as usize);
        let png_encoder = PngEncoder::new(&mut png_bytes);
        png_encoder.encode(&blipped.as_raw(), W, H, ColorType::Rgba8)?;

        let builder = MessageBuilder::new().attachment("higherlower.png", png_bytes);

        let mut message = HL_IMAGE_CHANNEL_ID
            .create_message(ctx, &builder)
            .await?
            .model()
            .await?;

        return Ok(message.attachments.pop().unwrap().url);
    }
}

struct HlGameStateInfo {
    user_id: u32,
    username: Username,
    avatar: String,
    rank: u32,
    country_code: CountryCode,
    map_id: u32,
    map_string: String,
    mods: GameMods,
    pp: f32,
    combo: u32,
    max_combo: u32,
    score: u32,
    acc: f32,
    miss_count: u32,
    grade: Grade,
    cover: String,
}

impl PartialEq for HlGameStateInfo {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id && self.map_id == other.map_id
    }
}

impl HlGameStateInfo {
    fn player_string(&self) -> String {
        format!(
            ":flag_{}: {} (#{})",
            self.country_code.to_lowercase(),
            self.username,
            self.rank,
        )
    }

    fn play_string(&self, pp_visible: bool) -> String {
        format!(
            "**{} {}**\n{} {} • **{}%** • **{}x**/{}x {}• **{}pp**",
            self.map_string,
            get_mods(self.mods),
            grade_emote(self.grade),
            with_comma_int(self.score),
            self.acc,
            self.combo,
            self.max_combo,
            if self.miss_count > 0 {
                format!("• **{}{}** ", self.miss_count, Emote::Miss.text())
            } else {
                String::new()
            },
            if pp_visible {
                self.pp.to_string().into()
            } else {
                Cow::Borrowed("???")
            }
        )
    }
}

fn hl_components() -> Vec<Component> {
    let higher_button = Button {
        custom_id: Some("higher_button".to_owned()),
        disabled: false,
        emoji: None,
        label: Some("Higher".to_owned()),
        style: ButtonStyle::Success,
        url: None,
    };

    let lower_button = Button {
        custom_id: Some("lower_button".to_owned()),
        disabled: false,
        emoji: None,
        label: Some("Lower".to_owned()),
        style: ButtonStyle::Danger,
        url: None,
    };

    let button_row = ActionRow {
        components: vec![
            Component::Button(higher_button),
            Component::Button(lower_button),
        ],
    };

    vec![Component::ActionRow(button_row)]
}

fn give_up_components() -> Vec<Component> {
    let give_up_button = Button {
        custom_id: Some("give_up_button".to_owned()),
        disabled: false,
        emoji: None,
        label: Some("Give Up".to_owned()),
        style: ButtonStyle::Danger,
        url: None,
    };

    let button_row = ActionRow {
        components: vec![Component::Button(give_up_button)],
    };

    vec![Component::ActionRow(button_row)]
}

pub async fn handle_higher(
    ctx: Arc<Context>,
    mut component: MessageComponentInteraction,
) -> BotResult<()> {
    let user = component.user_id()?;

    if let Entry::Occupied(mut entry) = ctx.hl_games().entry(user) {
        let game = entry.get_mut();
        defer_update(&ctx, &mut component, game).await?;

        if game.id != component.message.id {
            return Ok(());
        }

        if !game.check_guess(HlGuess::Higher) {
            game_over(&ctx, &component, game).await?;
            entry.remove();
        } else {
            correct_guess(&ctx, &component, game).await?;
        }
    }

    Ok(())
}

pub async fn handle_lower(
    ctx: Arc<Context>,
    mut component: MessageComponentInteraction,
) -> BotResult<()> {
    let user = component.user_id()?;

    if let Entry::Occupied(mut entry) = ctx.hl_games().entry(user) {
        let game = entry.get_mut();
        defer_update(&ctx, &mut component, game).await?;

        if game.id != component.message.id {
            return Ok(());
        }

        if !game.check_guess(HlGuess::Lower) {
            game_over(&ctx, &component, game).await?;
            entry.remove();
        } else {
            correct_guess(&ctx, &component, game).await?;
        }
    }

    Ok(())
}

#[allow(unused)]
pub async fn handle_give_up(
    ctx: Arc<Context>,
    mut component: MessageComponentInteraction,
) -> BotResult<()> {
    let user = component.user_id()?;
    if let Some((_, game)) = ctx.hl_games().remove(&user) {
        defer_update(&ctx, &mut component, &game).await?;

        let content = "Successfully ended the previous game.\n\
                            Start a new game by using `/higherlower`";
        let embed = EmbedBuilder::new().description(content).build();
        let builder = MessageBuilder::new().embed(embed).components(Vec::new());
        component.update(&ctx, &builder).await?;
    }

    Ok(())
}

async fn correct_guess(
    ctx: &Context,
    component: &MessageComponentInteraction,
    game: &mut HlGameState,
) -> BotResult<()> {
    std::mem::swap(&mut game.previous, &mut game.next);
    game.next = random_play(&ctx, game.previous.pp, game.current_score).await?;

    while game.next == game.previous {
        game.next = random_play(&ctx, game.previous.pp, game.current_score).await?;
    }

    game.current_score += 1;
    let image = match game.create_image(&ctx).await {
        Ok(url) => url,
        Err(err) => {
            let report = Report::new(err).wrap_err("failed to create hl image");
            warn!("{report:?}");

            String::new()
        }
    };
    let embed = game.to_embed(image);
    let builder = MessageBuilder::new().embed(embed);
    component.update(&ctx, &builder).await?;

    Ok(())
}

async fn game_over(
    ctx: &Context,
    component: &MessageComponentInteraction,
    game: &HlGameState,
) -> BotResult<()> {
    let better_score = ctx
        .psql()
        .upsert_higherlower_highscore(
            game.player.get(),
            game.mode,
            game.current_score,
            game.highscore,
        )
        .await?;

    let title = "Game over!";

    let content = match better_score {
        true => {
            format!(
                "You achieved a total score of {}! \nThis is your new personal best!",
                game.current_score
            )
        }
        false => {
            format!(
                "You achieved a total score of {}! \n\
                This unfortunately did not beat your personal best score of {}!",
                game.current_score, game.highscore
            )
        }
    };

    let embed = EmbedBuilder::new()
        .title(title)
        .description(content)
        .color(RED)
        .build();

    sleep(Duration::from_secs(2)).await;
    let builder = MessageBuilder::new().embed(embed).components(Vec::new());
    component.update(&ctx, &builder).await?;

    Ok(())
}

async fn defer_update(
    ctx: &Context,
    component: &mut MessageComponentInteraction,
    game: &HlGameState,
) -> BotResult<()> {
    let mut embeds = mem::take(&mut component.message.embeds);
    if let Some(embed) = embeds.first_mut() {
        if let Some(field) = embed.fields.get_mut(1) {
            field.value.truncate(field.value.len() - 7);
            let _ = write!(field.value, "{}pp**", round(game.next.pp));
        }
        if let Some(footer) = embed.footer.as_mut() {
            write!(
                footer.text,
                " • {}pp {} • Retrieving next play...",
                round((game.previous.pp - game.next.pp).abs()),
                if game.previous.pp < game.next.pp {
                    "higher"
                } else {
                    "lower"
                }
            )?;
        }
    }

    let builder = MessageBuilder::new().embed(embeds.pop().unwrap()); // TODO
    component.callback(&ctx, builder).await?;

    // client
    //     .interaction_callback(component.id, &component.token, &response)
    //     .exec()
    //     .await?;

    Ok(())
}

enum HlGuess {
    Higher,
    Lower,
}
