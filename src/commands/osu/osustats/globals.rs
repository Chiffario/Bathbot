use crate::{
    custom_client::{OsuStatsOrder, OsuStatsParams, OsuStatsScore},
    embeds::{EmbedData, OsuStatsGlobalsEmbed},
    pagination::{OsuStatsGlobalsPagination, Pagination},
    util::{constants::OSU_API_ISSUE, matcher, numbers, osu::ModSelection, MessageExt},
    Args, BotResult, CommandData, Context, MessageBuilder, Name,
};

use rosu_v2::prelude::{GameMode, OsuError};
use std::{borrow::Cow, collections::BTreeMap, fmt::Write, sync::Arc};
use twilight_model::application::interaction::application_command::CommandDataOption;

pub(super) async fn _scores(
    ctx: Arc<Context>,
    data: CommandData<'_>,
    mut args: ScoresArgs,
) -> BotResult<()> {
    let author_id = data.author()?.id;

    let name = match args.username.take() {
        Some(name) => name,
        None => match ctx.get_link(author_id.0) {
            Some(name) => name,
            None => return super::require_link(&ctx, &data).await,
        },
    };

    // Retrieve user
    let user = match super::request_user(&ctx, name.as_str(), Some(args.mode)).await {
        Ok(user) => user,
        Err(OsuError::NotFound) => {
            let content = format!("User `{}` was not found", name);

            return data.error(&ctx, content).await;
        }
        Err(why) => {
            let _ = data.error(&ctx, OSU_API_ISSUE).await;

            return Err(why.into());
        }
    };

    let params = args.into_params(user.username.as_str().into());

    // Retrieve their top global scores
    let (scores, amount) = match ctx.clients.custom.get_global_scores(&params).await {
        Ok((scores, amount)) => (
            scores
                .into_iter()
                .enumerate()
                .collect::<BTreeMap<usize, OsuStatsScore>>(),
            amount,
        ),
        Err(why) => {
            let content = "Some issue with the osustats website, blame bade";
            let _ = data.error(&ctx, content).await;

            return Err(why.into());
        }
    };

    // Accumulate all necessary data
    let pages = numbers::div_euclid(5, amount);
    let embed_data = OsuStatsGlobalsEmbed::new(&user, &scores, amount, (1, pages)).await;

    let mut content = format!(
        "`Rank: {rank_min} - {rank_max}` ~ \
        `Acc: {acc_min}% - {acc_max}%` ~ \
        `Order: {order} {descending}`",
        acc_min = params.acc_min,
        acc_max = params.acc_max,
        rank_min = params.rank_min,
        rank_max = params.rank_max,
        order = params.order,
        descending = if params.descending { "Desc" } else { "Asc" },
    );

    if let Some(selection) = params.mods {
        let _ = write!(
            content,
            " ~ `Mods: {}`",
            match selection {
                ModSelection::Exact(mods) => mods.to_string(),
                ModSelection::Exclude(mods) => format!("Exclude {}", mods),
                ModSelection::Include(mods) => format!("Include {}", mods),
            },
        );
    }

    // Creating the embed
    let embed = embed_data.into_builder().build();
    let builder = MessageBuilder::new().content(content).embed(embed);
    let response_raw = data.create_message(&ctx, builder).await?;

    // Skip pagination if too few entries
    if scores.len() <= 5 {
        return Ok(());
    }

    let response = data.get_response(&ctx, response_raw).await?;

    // Pagination
    let pagination =
        OsuStatsGlobalsPagination::new(Arc::clone(&ctx), response, user, scores, amount, params);
    let owner = author_id;

    tokio::spawn(async move {
        if let Err(why) = pagination.start(&ctx, owner, 60).await {
            unwind_error!(warn, why, "Pagination error (osustatsglobals): {}")
        }
    });

    Ok(())
}

#[command]
#[short_desc("All scores of a player that are on a map's global leaderboard")]
#[long_desc(
    "Show all scores of a player that are on a map's global leaderboard.\n\
    Mods can be specified through the usual `+_`, `+_!`, `-_!` syntax.\n\
    There are also multiple options you can set by specifying `key=value`.\n\
    These are the keys with their values:\n\
    - `acc`: single number or two numbers of the form `a..b` e.g. `acc=97.5..98`\n\
    - `rank`: single integer or two integers of the form `a..b` e.g. `rank=2..45`\n\
    - `sort`: `acc`, `combo`, `date` (default), `misses`, `pp`, `rank`, or `score`\n\
    - `reverse`: `true` or `false` (default)\n\
    Check https://osustats.ppy.sh/ for more info."
)]
#[usage(
    "[username] [mods] [acc=[number..]number] [rank=[integer..]integer] \
    [sort=acc/combo/date/misses/pp/rank/score] [reverse=true/false]"
)]
#[example(
    "badewanne3 -dt! acc=97.5..99.5 rank=42 sort=pp reverse=true",
    "vaxei sort=rank rank=1..5 +hdhr"
)]
#[aliases("osg", "osustatsglobal")]
pub async fn osustatsglobals(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match ScoresArgs::args(&ctx, &mut args, GameMode::STD) {
                Ok(params) => _scores(ctx, CommandData::Message { msg, args, num }, params).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_osustats(ctx, command).await,
    }
}

#[command]
#[short_desc("All scores of a player that are on a map's global leaderboard")]
#[long_desc(
    "Show all scores of a player that are on a mania map's global leaderboard.\n\
    Mods can be specified through the usual `+_`, `+_!`, `-_!` syntax.\n\
    There are also multiple options you can set by specifying `key=value`.\n\
    These are the keys with their values:\n\
    - `acc`: single number or two numbers of the form `a..b` e.g. `acc=97.5..98`\n\
    - `rank`: single integer or two integers of the form `a..b` e.g. `rank=2..45`\n\
    - `sort`: `acc`, `combo`, `date` (default), `misses`, `pp`, `rank`, or `score`\n\
    - `reverse`: `true` or `false` (default)\n\
    Check https://osustats.ppy.sh/ for more info."
)]
#[usage(
    "[username] [mods] [acc=[number..]number] [rank=[integer..]integer] \
    [sort=acc/combo/date/misses/pp/rank/score] [reverse=true/false]"
)]
#[example(
    "badewanne3 -dt! acc=97.5..99.5 rank=42 sort=pp reverse=true",
    "vaxei sort=rank rank=1..5 +hdhr"
)]
#[aliases("osgm", "osustatsglobalmania")]
pub async fn osustatsglobalsmania(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match ScoresArgs::args(&ctx, &mut args, GameMode::MNA) {
                Ok(params) => _scores(ctx, CommandData::Message { msg, args, num }, params).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_osustats(ctx, command).await,
    }
}

#[command]
#[short_desc("All scores of a player that are on a map's global leaderboard")]
#[long_desc(
    "Show all scores of a player that are on a taiko map's global leaderboard.\n\
    Mods can be specified through the usual `+_`, `+_!`, `-_!` syntax.\n\
    There are also multiple options you can set by specifying `key=value`.\n\
    These are the keys with their values:\n\
    - `acc`: single number or two numbers of the form `a..b` e.g. `acc=97.5..98`\n\
    - `rank`: single integer or two integers of the form `a..b` e.g. `rank=2..45`\n\
    - `sort`: `acc`, `combo`, `date` (default), `misses`, `pp`, `rank`, or `score`\n\
    - `reverse`: `true` or `false` (default)\n\
    Check https://osustats.ppy.sh/ for more info."
)]
#[usage(
    "[username] [mods] [acc=[number..]number] [rank=[integer..]integer] \
    [sort=acc/combo/date/misses/pp/rank/score] [reverse=true/false]"
)]
#[example(
    "badewanne3 -dt! acc=97.5..99.5 rank=42 sort=pp reverse=true",
    "vaxei sort=rank rank=1..5 +hdhr"
)]
#[aliases("osgt", "osustatsglobaltaiko")]
pub async fn osustatsglobalstaiko(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match ScoresArgs::args(&ctx, &mut args, GameMode::TKO) {
                Ok(params) => _scores(ctx, CommandData::Message { msg, args, num }, params).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_osustats(ctx, command).await,
    }
}

#[command]
#[short_desc("All scores of a player that are on a map's global leaderboard")]
#[long_desc(
    "Show all scores of a player that are on a ctb map's global leaderboard.\n\
    Mods can be specified through the usual `+_`, `+_!`, `-_!` syntax.\n\
    There are also multiple options you can set by specifying `key=value`.\n\
    These are the keys with their values:\n\
    - `acc`: single number or two numbers of the form `a..b` e.g. `acc=97.5..98`\n\
    - `rank`: single integer or two integers of the form `a..b` e.g. `rank=2..45`\n\
    - `sort`: `acc`, `combo`, `date` (default), `misses`, `pp`, `rank`, or `score`\n\
    - `reverse`: `true` or `false` (default)\n\
    Check https://osustats.ppy.sh/ for more info."
)]
#[usage(
    "[username] [mods] [acc=[number..]number] [rank=[integer..]integer] \
    [sort=acc/combo/date/misses/pp/rank/score] [reverse=true/false]"
)]
#[example(
    "badewanne3 -dt! acc=97.5..99.5 rank=42 sort=pp reverse=true",
    "vaxei sort=rank rank=1..5 +hdhr"
)]
#[aliases("osgc", "osustatsglobalctb")]
pub async fn osustatsglobalsctb(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match ScoresArgs::args(&ctx, &mut args, GameMode::CTB) {
                Ok(params) => _scores(ctx, CommandData::Message { msg, args, num }, params).await,
                Err(content) => msg.error(&ctx, content).await,
            }
        }
        CommandData::Interaction { command } => super::slash_osustats(ctx, command).await,
    }
}

pub(super) struct ScoresArgs {
    pub username: Option<Name>,
    pub mode: GameMode,
    pub rank_min: usize,
    pub rank_max: usize,
    pub acc_min: f32,
    pub acc_max: f32,
    pub order: OsuStatsOrder,
    pub mods: Option<ModSelection>,
    pub descending: bool,
}

impl ScoresArgs {
    const MIN_RANK: usize = 1;
    const MAX_RANK: usize = 100;

    const ERR_PARSE_ACC: &'static str = "Failed to parse `accuracy`.\n\
        Must be either decimal number \
        or two decimal numbers of the form `a..b` e.g. `97.5..98.5`.";

    const ERR_PARSE_RANK: &'static str = "Failed to parse `rank`.\n\
        Must be either a positive integer \
        or two positive integers of the form `a..b` e.g. `2..45`.";

    const ERR_PARSE_MODS: &'static str = "Failed to parse mods.\n\
        If you want included mods, specify it e.g. as `+hrdt`.\n\
        If you want exact mods, specify it e.g. as `+hdhr!`.\n\
        And if you want to exclude mods, specify it e.g. as `-hdnf!`.";

    fn into_params(self, username: Name) -> OsuStatsParams {
        OsuStatsParams {
            username,
            mode: self.mode,
            page: 1,
            rank_min: self.rank_min,
            rank_max: self.rank_max,
            acc_min: self.acc_min,
            acc_max: self.acc_max,
            order: self.order,
            mods: self.mods,
            descending: self.descending,
        }
    }

    fn args(ctx: &Context, args: &mut Args, mode: GameMode) -> Result<Self, Cow<'static, str>> {
        let mut username = None;
        let mut rank_min = None;
        let mut rank_max = None;
        let mut acc_min = None;
        let mut acc_max = None;
        let mut order = None;
        let mut mods = None;
        let mut descending = None;

        for arg in args {
            if let Some(idx) = arg.find('=').filter(|&i| i > 0) {
                let key = &arg[..idx];
                let value = arg[idx + 1..].trim_end();

                match key {
                    "acc" | "accuracy" | "a" => match value.find("..") {
                        Some(idx) => {
                            let bot = &value[..idx];
                            let top = &value[idx + 2..];

                            let min = if bot.is_empty() {
                                0.0
                            } else if let Ok(num) = bot.parse::<f32>() {
                                num.max(0.0).min(100.0)
                            } else {
                                return Err(Self::ERR_PARSE_ACC.into());
                            };

                            let max = if top.is_empty() {
                                100.0
                            } else if let Ok(num) = top.parse::<f32>() {
                                num.max(0.0).min(100.0)
                            } else {
                                return Err(Self::ERR_PARSE_ACC.into());
                            };

                            acc_min = Some(min.min(max));
                            acc_max = Some(min.max(max));
                        }
                        None => acc_min = Some(value.parse().map_err(|_| Self::ERR_PARSE_ACC)?),
                    },
                    "rank" | "r" => match value.find("..") {
                        Some(idx) => {
                            let bot = &value[..idx];
                            let top = &value[idx + 2..];

                            let min = if bot.is_empty() {
                                Self::MIN_RANK
                            } else if let Ok(num) = bot.parse::<usize>() {
                                num.max(Self::MIN_RANK).min(Self::MAX_RANK)
                            } else {
                                return Err(Self::ERR_PARSE_RANK.into());
                            };

                            let max = if top.is_empty() {
                                Self::MAX_RANK
                            } else if let Ok(num) = top.parse::<usize>() {
                                num.max(Self::MIN_RANK).min(Self::MAX_RANK)
                            } else {
                                return Err(Self::ERR_PARSE_RANK.into());
                            };

                            rank_min = Some(min.min(max));
                            rank_max = Some(min.max(max));
                        }
                        None => rank_max = Some(value.parse().map_err(|_| Self::ERR_PARSE_RANK)?),
                    },
                    "sort" | "s" | "order" | "ordering" => match value {
                        "date" | "d" | "scoredate" => order = Some(OsuStatsOrder::PlayDate),
                        "pp" => order = Some(OsuStatsOrder::Pp),
                        "rank" | "r" => order = Some(OsuStatsOrder::Rank),
                        "acc" | "accuracy" | "a" => order = Some(OsuStatsOrder::Accuracy),
                        "combo" | "c" => order = Some(OsuStatsOrder::Combo),
                        "score" | "s" => order = Some(OsuStatsOrder::Score),
                        "misses" | "miss" | "m" => order = Some(OsuStatsOrder::Misses),
                        _ => {
                            let content = "Failed to parse `sort`.\n\
                                Must be either `acc`, `combo`, `date`, `misses`, `pp`, `rank`, or `score`.";

                            return Err(content.into());
                        }
                    },
                    "reverse" => match value {
                        "true" | "1" => descending = Some(false),
                        "false" | "0" => descending = Some(true),
                        _ => {
                            let content =
                                "Failed to parse `reverse`. Must be either `true` or `false`.";

                            return Err(content.into());
                        }
                    },
                    "mods" => match matcher::get_mods(&value) {
                        Some(mods_) => mods = Some(mods_),
                        None => return Err(Self::ERR_PARSE_MODS.into()),
                    },
                    _ => {
                        let content = format!(
                            "Unrecognized option `{}`.\n\
                            Available options are: `acc`, `rank`, `sort`, or `reverse`.",
                            key
                        );

                        return Err(content.into());
                    }
                }
            } else if let Some(mods_) = matcher::get_mods(arg.as_ref()) {
                mods = Some(mods_);
            } else {
                username = Some(Args::try_link_name(ctx, arg)?);
            }
        }

        let args = Self {
            username,
            mode,
            rank_min: rank_min.unwrap_or(Self::MIN_RANK),
            rank_max: rank_max.unwrap_or(Self::MAX_RANK),
            acc_min: acc_min.unwrap_or(0.0),
            acc_max: acc_max.unwrap_or(100.0),
            order: order.unwrap_or_default(),
            mods,
            descending: descending.unwrap_or(true),
        };

        Ok(args)
    }

    pub(super) fn slash(
        ctx: &Context,
        options: Vec<CommandDataOption>,
    ) -> BotResult<Result<Self, Cow<'static, str>>> {
        let mut username = None;
        let mut rank_min = None;
        let mut rank_max = None;
        let mut acc_min = None;
        let mut acc_max = None;
        let mut order = None;
        let mut mods = None;
        let mut descending = None;
        let mut mode = None;

        for option in options {
            match option {
                CommandDataOption::String { name, value } => match name.as_str() {
                    "mode" => mode = parse_mode_option!(value, "osustats scores"),
                    "mods" => match matcher::get_mods(&value) {
                        Some(mods_) => mods = Some(mods_),
                        None => return Ok(Err(Self::ERR_PARSE_MODS.into())),
                    },
                    "sort" => match value.as_str() {
                        "acc" => order = Some(OsuStatsOrder::Accuracy),
                        "combo" => order = Some(OsuStatsOrder::Combo),
                        "misses" => order = Some(OsuStatsOrder::Misses),
                        "pp" => order = Some(OsuStatsOrder::Pp),
                        "rank" => order = Some(OsuStatsOrder::Rank),
                        "score" => order = Some(OsuStatsOrder::Score),
                        "date" => order = Some(OsuStatsOrder::PlayDate),
                        _ => bail_cmd_option!("osustats scores sort", string, value),
                    },
                    "name" => username = Some(value.into()),
                    "discord" => username = parse_discord_option!(ctx, value, "osustats scores"),
                    _ => bail_cmd_option!("osustats scores", string, name),
                },
                CommandDataOption::Integer { name, value } => match name.as_str() {
                    "rank_min" => {
                        rank_min =
                            Some((value.max(Self::MIN_RANK as i64) as usize).min(Self::MAX_RANK))
                    }
                    "rank_max" => {
                        rank_max =
                            Some((value.max(Self::MIN_RANK as i64) as usize).min(Self::MAX_RANK))
                    }
                    _ => bail_cmd_option!("osustats scores", integer, name),
                },
                CommandDataOption::Boolean { name, value } => match name.as_str() {
                    "reverse" => descending = Some(!value),
                    _ => bail_cmd_option!("osustats scores", boolean, name),
                },
                CommandDataOption::SubCommand { name, .. } => {
                    bail_cmd_option!("osustats scores", subcommand, name)
                }
            }
        }

        let args = Self {
            username,
            mode: mode.unwrap_or(GameMode::STD),
            rank_min: rank_min.unwrap_or(Self::MIN_RANK),
            rank_max: rank_max.unwrap_or(Self::MAX_RANK),
            acc_min: acc_min.unwrap_or(0.0),
            acc_max: acc_max.unwrap_or(100.0),
            order: order.unwrap_or_default(),
            mods,
            descending: descending.unwrap_or(true),
        };

        Ok(Ok(args))
    }
}