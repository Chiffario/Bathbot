use super::ErrorType;
use crate::{
    embeds::{EmbedData, TopIfEmbed},
    pagination::{Pagination, TopIfPagination},
    pp::{Calculations, PPCalculator},
    tracking::process_tracking,
    util::{
        constants::{GENERAL_ISSUE, OSU_API_ISSUE},
        matcher, numbers,
        osu::ModSelection,
        MessageExt,
    },
    Args, BotResult, CommandData, Context, Error, MessageBuilder, Name,
};

use futures::{
    future::TryFutureExt,
    stream::{FuturesUnordered, TryStreamExt},
};
use rosu_v2::prelude::{GameMode, GameMods, OsuError, Score};
use std::{borrow::Cow, cmp::Ordering, fmt::Write, sync::Arc};
use twilight_model::application::interaction::application_command::CommandDataOption;

const NM: GameMods = GameMods::NoMod;
const DT: GameMods = GameMods::DoubleTime;
const NC: GameMods = GameMods::NightCore;
const HT: GameMods = GameMods::HalfTime;
const EZ: GameMods = GameMods::Easy;
const HR: GameMods = GameMods::HardRock;
const PF: GameMods = GameMods::Perfect;
const SD: GameMods = GameMods::SuddenDeath;

pub(super) async fn _topif(
    ctx: Arc<Context>,
    data: CommandData<'_>,
    args: IfArgs,
) -> BotResult<()> {
    let IfArgs { name, mode, mods } = args;

    let author_id = data.author()?.id;

    let name = match name {
        Some(name) => name,
        None => match ctx.get_link(author_id.0) {
            Some(name) => name,
            None => return super::require_link(&ctx, &data).await,
        },
    };

    if let ModSelection::Exact(mods) | ModSelection::Include(mods) = mods {
        let mut content = None;
        let ezhr = EZ | HR;
        let dtht = DT | HT;

        if mods & ezhr == ezhr {
            content = Some("Looks like an invalid mod combination, EZ and HR exclude each other.");
        }

        if mods & dtht == dtht {
            content = Some("Looks like an invalid mod combination, DT and HT exclude each other");
        }

        if let Some(content) = content {
            return data.error(&ctx, content).await;
        }
    }

    // Retrieve the user and their top scores
    let user_fut = super::request_user(&ctx, &name, Some(mode)).map_err(From::from);
    let scores_fut = ctx
        .osu()
        .user_scores(name.as_str())
        .best()
        .mode(mode)
        .limit(100);

    let scores_fut = super::prepare_scores(&ctx, scores_fut);

    let (user, mut scores) = match tokio::try_join!(user_fut, scores_fut) {
        Ok((user, scores)) => (user, scores),
        Err(ErrorType::Osu(OsuError::NotFound)) => {
            let content = format!("User `{}` was not found", name);

            return data.error(&ctx, content).await;
        }
        Err(ErrorType::Osu(why)) => {
            let _ = data.error(&ctx, OSU_API_ISSUE).await;

            return Err(why.into());
        }
        Err(ErrorType::Bot(why)) => {
            let _ = data.error(&ctx, GENERAL_ISSUE).await;

            return Err(why);
        }
    };

    // Process user and their top scores for tracking
    process_tracking(&ctx, mode, &mut scores, Some(&user)).await;

    // Calculate bonus pp
    let actual_pp: f32 = scores
        .iter()
        .filter_map(|s| s.weight)
        .map(|weight| weight.pp)
        .sum();

    let bonus_pp = user.statistics.as_ref().unwrap().pp - actual_pp;
    let arg_mods = args.mods;

    // Modify scores
    let scores_fut = scores
        .into_iter()
        .enumerate()
        .map(|(i, mut score)| async move {
            let map = score.map.as_ref().unwrap();

            if map.convert {
                return Ok((i + 1, score, None));
            }

            let changed = match arg_mods {
                ModSelection::Exact(mods) => {
                    let changed = score.mods != mods;
                    score.mods = mods;

                    changed
                }
                ModSelection::Exclude(mut mods) if mods != NM => {
                    if mods.contains(DT) {
                        mods |= NC;
                    }

                    if mods.contains(SD) {
                        mods |= PF
                    }

                    let changed = score.mods.intersects(mods);
                    score.mods.remove(mods);

                    changed
                }
                ModSelection::Include(mods) if mods != NM => {
                    let mut changed = false;

                    if mods.contains(DT) && score.mods.contains(HT) {
                        score.mods.remove(HT);
                        changed = true;
                    }

                    if mods.contains(HT) && score.mods.contains(DT) {
                        score.mods.remove(NC);
                        changed = true;
                    }

                    if mods.contains(HR) && score.mods.contains(EZ) {
                        score.mods.remove(EZ);
                        changed = true;
                    }

                    if mods.contains(EZ) && score.mods.contains(HR) {
                        score.mods.remove(HR);
                        changed = true;
                    }

                    changed |= !score.mods.contains(mods);
                    score.mods.insert(mods);

                    changed
                }
                _ => false,
            };

            let mut calculations = Calculations::STARS | Calculations::MAX_PP;

            if changed {
                score.grade = score.grade(Some(score.accuracy));
                calculations |= Calculations::PP;
            }

            let mut calculator = PPCalculator::new().score(&score).map(map);

            calculator.calculate(calculations).await?;

            let max_pp = calculator.max_pp().unwrap_or(0.0);
            let (stars, pp) = (calculator.stars(), calculator.pp());

            drop(calculator);

            if let Some(stars) = stars {
                score.map.as_mut().unwrap().stars = stars;
            }

            if let Some(pp) = pp {
                score.pp.replace(pp);
            }

            Ok((i + 1, score, Some(max_pp)))
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect();

    let mut scores_data: Vec<_> = match scores_fut.await {
        Ok(scores) => scores,
        Err(why) => {
            let _ = data.error(&ctx, GENERAL_ISSUE).await;

            return Err(why);
        }
    };

    // Sort by adjusted pp
    scores_data.sort_unstable_by(|(_, s1, _), (_, s2, _)| {
        s2.pp.partial_cmp(&s1.pp).unwrap_or(Ordering::Equal)
    });

    // Calculate adjusted pp
    let adjusted_pp: f32 = scores_data
        .iter()
        .map(|(i, Score { pp, .. }, ..)| pp.unwrap_or(0.0) * 0.95_f32.powi(*i as i32 - 1))
        .sum();

    let adjusted_pp = numbers::round((bonus_pp + adjusted_pp).max(0.0) as f32);

    // Accumulate all necessary data
    let content = match args.mods {
        ModSelection::Exact(mods) => format!(
            "`{name}`{plural} {mode}top100 with only `{mods}` scores:",
            name = user.username,
            plural = plural(user.username.as_str()),
            mode = mode_str(mode),
            mods = mods
        ),
        ModSelection::Exclude(mods) if mods != NM => {
            let mods: Vec<_> = mods.iter().collect();
            let len = mods.len();
            let mut mod_iter = mods.into_iter();
            let mut mod_str = String::with_capacity(len * 6 - 2);

            if let Some(first) = mod_iter.next() {
                let last = mod_iter.next_back();
                let _ = write!(mod_str, "`{}`", first);

                for elem in mod_iter {
                    let _ = write!(mod_str, ", `{}`", elem);
                }

                if let Some(last) = last {
                    let _ = match len {
                        2 => write!(mod_str, " and `{}`", last),
                        _ => write!(mod_str, ", and `{}`", last),
                    };
                }
            }
            format!(
                "`{name}`{plural} {mode}top100 without {mods}:",
                name = user.username,
                plural = plural(user.username.as_str()),
                mode = mode_str(mode),
                mods = mod_str
            )
        }
        ModSelection::Include(mods) if mods != NM => format!(
            "`{name}`{plural} {mode}top100 with `{mods}` inserted everywhere:",
            name = user.username,
            plural = plural(user.username.as_str()),
            mode = mode_str(mode),
            mods = mods,
        ),
        _ => format!(
            "`{name}`{plural} top {mode}scores:",
            name = user.username,
            plural = plural(user.username.as_str()),
            mode = mode_str(mode),
        ),
    };

    let pages = numbers::div_euclid(5, scores_data.len());
    let iter = scores_data.iter().take(5);
    let pre_pp = user.statistics.as_ref().unwrap().pp;
    let embed_data_fut = TopIfEmbed::new(&user, iter, mode, pre_pp, adjusted_pp, (1, pages));

    // Creating the embed
    let embed = embed_data_fut.await.into_builder().build();
    let builder = MessageBuilder::new().content(content).embed(embed);
    let response_raw = data.create_message(&ctx, builder).await?;

    // * Don't add maps of scores to DB since their stars were potentially changed

    // Skip pagination if too few entries
    if scores_data.len() <= 5 {
        return Ok(());
    }

    let response = data.get_response(&ctx, response_raw).await?;

    // Pagination
    let pre_pp = user.statistics.as_ref().unwrap().pp;
    let pagination = TopIfPagination::new(response, user, scores_data, mode, pre_pp, adjusted_pp);
    let owner = author_id;

    tokio::spawn(async move {
        if let Err(why) = pagination.start(&ctx, owner, 60).await {
            unwind_error!(warn, why, "Pagination error (topif): {}")
        }
    });

    Ok(())
}

#[command]
#[short_desc("Display a user's top plays with(out) the given mods")]
#[long_desc(
    "Display how a user's top plays would look like with the given mods.\n\
    As for all other commands with mods input, you can specify them as follows:\n  \
    - `+mods` to include the mod(s) into all scores\n  \
    - `+mods!` to make all scores have exactly those mods\n  \
    - `-mods!` to remove all these mods from all scores"
)]
#[usage("[username] [mods]")]
#[example("badewanne3 -hd!", "+hdhr!", "whitecat +hddt")]
#[aliases("ti")]
pub async fn topif(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match IfArgs::args(&ctx, &mut args, GameMode::STD) {
                Ok(if_args) => _topif(ctx, CommandData::Message { msg, args, num }, if_args).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, command).await,
    }
}

#[command]
#[short_desc("Display a user's top taiko plays with(out) the given mods")]
#[long_desc(
    "Display how a user's top taiko plays would look like with the given mods.\n\
    As for all other commands with mods input, you can specify them as follows:\n  \
    - `+mods` to include the mod(s) into all scores\n  \
    - `+mods!` to make all scores have exactly those mods\n  \
    - `-mods!` to remove all these mods from all scores\n\
    To exclude converts, specify `-convert` / `-c` as last argument."
)]
#[usage("[username] [mods] [-c]")]
#[example("badewanne3 -hd!", "+hdhr! -c", "whitecat +hddt")]
#[aliases("tit")]
pub async fn topiftaiko(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match IfArgs::args(&ctx, &mut args, GameMode::TKO) {
                Ok(if_args) => _topif(ctx, CommandData::Message { msg, args, num }, if_args).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, command).await,
    }
}

#[command]
#[short_desc("Display a user's top ctb plays with(out) the given mods")]
#[long_desc(
    "Display how a user's top ctb plays would look like with the given mods.\n\
    As for all other commands with mods input, you can specify them as follows:\n  \
    - `+mods` to include the mod(s) into all scores\n  \
    - `+mods!` to make all scores have exactly those mods\n  \
    - `-mods!` to remove all these mods from all scores\n\
    To exclude converts, specify `-convert` / `-c` as last argument."
)]
#[usage("[username] [mods] [-c]")]
#[example("badewanne3 -hd!", "+hdhr! -c", "whitecat +hddt")]
#[aliases("tic")]
pub async fn topifctb(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match IfArgs::args(&ctx, &mut args, GameMode::CTB) {
                Ok(if_args) => _topif(ctx, CommandData::Message { msg, args, num }, if_args).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, command).await,
    }
}

fn plural(name: &str) -> &'static str {
    match name.chars().last() {
        Some('s') => "'",
        Some(_) | None => "'s",
    }
}

fn mode_str(mode: GameMode) -> &'static str {
    match mode {
        GameMode::STD => "",
        GameMode::TKO => "taiko ",
        GameMode::CTB => "ctb ",
        GameMode::MNA => "mania ",
    }
}

pub(super) struct IfArgs {
    name: Option<Name>,
    mode: GameMode,
    mods: ModSelection,
}

impl IfArgs {
    const ERR_PARSE_MODS: &'static str = "Failed to parse mods.\n\
        If you want to insert mods everywhere, specify it e.g. as `+hrdt`.\n\
        If you want to replace mods everywhere, specify it e.g. as `+hdhr!`.\n\
        And if you want to remote mods everywhere, specify it e.g. as `-hdnf!`.";

    fn args(ctx: &Context, args: &mut Args, mode: GameMode) -> Result<Self, &'static str> {
        let mut name = None;
        let mut mods = None;

        for arg in args.take(2) {
            match matcher::get_mods(arg) {
                Some(mods_) => mods = Some(mods_),
                None => name = Some(Args::try_link_name(ctx, arg)?),
            }
        }

        let mods = mods.ok_or(Self::ERR_PARSE_MODS)?;

        Ok(Self { name, mode, mods })
    }

    pub(super) fn slash(
        ctx: &Context,
        options: Vec<CommandDataOption>,
    ) -> BotResult<Result<Self, Cow<'static, str>>> {
        let mut username = None;
        let mut mods = None;
        let mut mode = None;

        for option in options {
            match option {
                CommandDataOption::String { name, value } => match name.as_str() {
                    "name" => username = Some(value.into()),
                    "mods" => match matcher::get_mods(&value) {
                        Some(mods_) => mods = Some(mods_),
                        None => return Ok(Err(Self::ERR_PARSE_MODS.into())),
                    },
                    "mode" => mode = parse_mode_option!(value, "top if"),
                    "discord" => username = parse_discord_option!(ctx, value, "top if"),
                    _ => bail_cmd_option!("top if", string, name),
                },
                CommandDataOption::Integer { name, .. } => {
                    bail_cmd_option!("top if", integer, name)
                }
                CommandDataOption::Boolean { name, .. } => {
                    bail_cmd_option!("top if", boolean, name)
                }
                CommandDataOption::SubCommand { name, .. } => {
                    bail_cmd_option!("top if", subcommand, name)
                }
            }
        }

        let args = Self {
            mods: mods.ok_or(Error::InvalidCommandOptions)?,
            name: username,
            mode: mode.unwrap_or(GameMode::STD),
        };

        Ok(Ok(args))
    }
}