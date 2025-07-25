use std::{
    borrow::Cow,
    cmp::Ordering,
    fmt::{Display, Formatter, Result as FmtResult, Write},
    time::Duration,
};

use bathbot_model::embed_builder::{
    EmoteTextValue, HitresultsValue, MapperValue, ScoreEmbedSettings, SettingValue, SettingsImage,
    Value,
};
use bathbot_psql::model::configs::ScoreData;
use bathbot_util::{
    AuthorBuilder, Authored, BucketName, CowUtils, EmbedBuilder, FooterBuilder, MessageBuilder,
    ModsFormatter, attachment,
    constants::{GENERAL_ISSUE, ORDR_ISSUE, OSU_API_ISSUE, OSU_BASE},
    datetime::{HowLongAgoDynamic, HowLongAgoText, SHORT_NAIVE_DATETIME_FORMAT, SecToMinSec},
    fields,
    numbers::round,
};
use eyre::{Report, Result};
use rosu_pp::model::beatmap::BeatmapAttributes;
use rosu_render::{ClientError as OrdrError, client::error::ApiError as OrdrApiError};
use rosu_v2::{
    error::OsuError,
    model::{GameMode, Grade},
    prelude::{GameMod, GameMods, RankStatus},
};
use time::OffsetDateTime;
use twilight_model::{
    channel::message::{
        Component, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::Permissions,
    id::{
        Id,
        marker::{ChannelMarker, GuildMarker, MessageMarker, UserMarker},
    },
};

use crate::{
    active::{
        ActiveMessages, BuildPage, ComponentResult, IActiveMessage,
        impls::{CachedRender, embed_builder::ValueKind},
        pagination::{Pages, handle_pagination_component, handle_pagination_modal},
    },
    commands::{
        osu::{OngoingRender, ProgressResponse, RENDERER_NAME, RenderStatus, RenderStatusInner},
        utility::{ScoreEmbedData, ScoreEmbedDataWrap},
    },
    core::{Context, commands::OwnedCommandOrigin},
    embeds::HitResultFormatter,
    manager::{ReplayError, redis::osu::CachedUser},
    util::{
        CachedUserExt, Emote, MessageExt,
        interaction::{InteractionComponent, InteractionModal},
        osu::{GradeFormatter, ScoreFormatter},
    },
};

pub struct SingleScorePagination {
    pub settings: ScoreEmbedSettings,
    scores: Box<[ScoreEmbedDataWrap]>,
    score_data: ScoreData,
    msg_owner: Id<UserMarker>,
    pages: Pages,

    author: AuthorBuilder,
    content: SingleScoreContent,
}

impl SingleScorePagination {
    pub const IMAGE_H: u32 = 170;
    pub const IMAGE_NAME: &'static str = "map_graph.png";
    pub const IMAGE_W: u32 = 590;

    pub fn new(
        user: &CachedUser,
        scores: Box<[ScoreEmbedDataWrap]>,
        settings: ScoreEmbedSettings,
        score_data: ScoreData,
        msg_owner: Id<UserMarker>,
        content: SingleScoreContent,
    ) -> Self {
        let pages = Pages::new(1, scores.len());

        Self {
            settings,
            scores,
            score_data,
            msg_owner,
            pages,
            author: user.author_builder(false),
            content,
        }
    }

    pub fn set_index(&mut self, idx: usize) {
        self.pages.set_index(idx);
    }

    // refactored into a pub method so it's usable from elsewhere
    pub async fn async_build_page(
        &mut self,
        content: Box<str>,
        mark_idx: MarkIndex,
    ) -> Result<BuildPage> {
        let score = &*self.scores[self.pages.index()].get_mut().await?;

        let embed = Self::apply_settings(&self.settings, score, self.score_data, mark_idx);

        let url = format!("{OSU_BASE}b/{}", score.map.map_id());

        #[allow(unused_mut)]
        let mut description = if score.pb_idx.is_some() || score.global_idx.is_some() {
            let mut description = String::with_capacity(25);
            description.push_str("__**");

            if let Some(pb_idx) = &score.pb_idx {
                description.push_str(&pb_idx.formatted);

                if score.global_idx.is_some() {
                    description.reserve(19);
                    description.push_str(" and ");
                }
            }

            if let Some(idx) = score.global_idx {
                let _ = write!(description, "Global Top #{idx}");
            }

            description.push_str("**__");

            description
        } else {
            String::new()
        };

        #[cfg(feature = "twitch")]
        if let Some(ref data) = score.twitch {
            if !description.is_empty() {
                description.push(' ');
            }

            data.append_to_description(&score.score, &score.map, &mut description);
        }

        let builder = embed
            .author(self.author.clone())
            .description(description)
            .url(url);

        Ok(BuildPage::new(builder, false).content(content))
    }

    pub fn apply_settings(
        settings: &ScoreEmbedSettings,
        data: &ScoreEmbedData,
        score_data: ScoreData,
        mark_idx: MarkIndex,
    ) -> EmbedBuilder {
        apply_settings(settings, data, score_data, mark_idx)
    }

    async fn handle_miss_analyzer_button(
        &mut self,
        component: &InteractionComponent,
    ) -> ComponentResult {
        let data = match self.scores[self.pages.index()].get_mut().await {
            Ok(data) => data,
            Err(err) => return ComponentResult::Err(err),
        };

        let score_id = match data.miss_analyzer.take() {
            Some(miss_analyzer) => miss_analyzer.score_id,
            None => return ComponentResult::Err(eyre!("Unexpected miss analyzer component")),
        };

        let Some(guild) = component.guild_id.map(Id::get) else {
            return ComponentResult::Err(eyre!("Missing guild id for miss analyzer button"));
        };

        let channel = component.channel_id.get();
        let msg = component.message.id.get();

        debug!(
            score_id,
            msg, channel, guild, "Sending message to miss analyzer",
        );

        let res_fut = Context::client().miss_analyzer_score_response(guild, channel, msg, score_id);

        if let Err(err) = res_fut.await {
            warn!(?err, "Failed to send miss analyzer response");
        }

        ComponentResult::BuildPage
    }

    async fn handle_render_button(&mut self, component: &InteractionComponent) -> ComponentResult {
        let data = match self.scores[self.pages.index()].get_mut().await {
            Ok(data) => data,
            Err(err) => return ComponentResult::Err(err),
        };

        let Some(score_id) = data.replay_score_id.take() else {
            return ComponentResult::Err(eyre!("Unexpected render component"));
        };

        let owner = match component.user_id() {
            Ok(user_id) => user_id,
            Err(err) => return ComponentResult::Err(err),
        };

        // Check if the score id has already been rendered
        match Context::replay().get_video_url(score_id).await {
            Ok(Some(video_url)) => {
                let channel_id = component.message.channel_id;

                // Spawn in new task so that we're sure to callback the component in time
                tokio::spawn(async move {
                    let cached = CachedRender::new(score_id, video_url, true, owner);
                    let begin_fut = ActiveMessages::builder(cached).begin(channel_id);

                    if let Err(err) = begin_fut.await {
                        error!(?err, "Failed to begin cached render message");
                    }
                });

                return ComponentResult::BuildPage;
            }
            Ok(None) => {}
            Err(err) => warn!(?err),
        }

        if let Some(cooldown) = Context::check_ratelimit(owner, BucketName::Render) {
            // Put the replay back so that the button can still be used
            data.replay_score_id = Some(score_id);

            return self.render_cooldown_response(component, cooldown).await;
        }

        tokio::spawn(Self::render_response(
            (component.message.id, component.message.channel_id),
            component.permissions,
            score_id,
            owner,
            component.guild_id,
        ));

        ComponentResult::BuildPage
    }

    async fn render_cooldown_response(
        &mut self,
        component: &InteractionComponent,
        cooldown: i64,
    ) -> ComponentResult {
        let content = format!(
            "Rendering is on cooldown for you <@{owner}>, try again in {cooldown} seconds",
            owner = self.msg_owner
        );

        let embed = EmbedBuilder::new().description(content).color_red();
        let builder = MessageBuilder::new().embed(embed);

        let reply_fut = component.message.reply(builder, component.permissions);

        match reply_fut.await {
            Ok(_) => ComponentResult::BuildPage,
            Err(err) => {
                let wrap = "Failed to reply for render cooldown error";

                ComponentResult::Err(Report::new(err).wrap_err(wrap))
            }
        }
    }

    async fn render_response(
        orig: (Id<MessageMarker>, Id<ChannelMarker>),
        permissions: Option<Permissions>,
        score_id: u64,
        owner: Id<UserMarker>,
        guild: Option<Id<GuildMarker>>,
    ) {
        let mut status = RenderStatus::new_preparing_replay();

        let msg = match orig.reply(status.as_message(), permissions).await {
            Ok(response) => match response.model().await {
                Ok(msg) => msg,
                Err(err) => return error!(?err, "Failed to get reply after render button click"),
            },
            Err(err) => return error!(?err, "Failed to reply after render button click"),
        };

        status.set(RenderStatusInner::PreparingReplay);

        if let Some(update_fut) = msg.update(status.as_message(), permissions) {
            let _ = update_fut.await;
        }

        let replay_manager = Context::replay();
        let replay_fut = replay_manager.get_replay(score_id);
        let settings_fut = replay_manager.get_settings(owner);

        let (replay_res, settings_res) = tokio::join!(replay_fut, settings_fut);

        let replay = match replay_res {
            Ok(Some(replay)) => replay,
            Ok(None) => {
                let content = "Looks like the replay for that score is not available";

                let embed = EmbedBuilder::new().color_red().description(content);
                let builder = MessageBuilder::new().embed(embed);

                return match msg.update(builder, permissions) {
                    Some(update_fut) => match update_fut.await {
                        Ok(_) => {}
                        Err(err) => error!(?err, "Failed to update message"),
                    },
                    None => warn!("Lacking permission to update message on error"),
                };
            }
            Err(err) => {
                let content = match err {
                    ReplayError::AlreadyRequestedCheck(err) => {
                        error!(?err, "{}", ReplayError::ALREADY_REQUESTED_TEXT);

                        GENERAL_ISSUE
                    }
                    ReplayError::Osu(OsuError::NotFound) => "Found no score with that id",
                    ReplayError::Osu(err) => {
                        error!(err = ?Report::new(err), "Failed to get replay");

                        OSU_API_ISSUE
                    }
                };

                let embed = EmbedBuilder::new().color_red().description(content);
                let builder = MessageBuilder::new().embed(embed);

                if let Some(update_fut) = msg.update(builder, permissions) {
                    let _ = update_fut.await;
                }

                return;
            }
        };

        let settings = match settings_res {
            Ok(settings) => settings,
            Err(err) => {
                let embed = EmbedBuilder::new().color_red().description(GENERAL_ISSUE);
                let builder = MessageBuilder::new().embed(embed);

                if let Some(update_fut) = msg.update(builder, permissions) {
                    let _ = update_fut.await;
                }

                return error!(?err);
            }
        };

        status.set(RenderStatusInner::CommissioningRender);

        let response = match msg.update(status.as_message(), permissions) {
            Some(update_fut) => match update_fut.await {
                Ok(response) => match response.model().await {
                    Ok(msg) => Some(msg),
                    Err(err) => {
                        warn!(err = ?Report::new(err), "Failed to deserialize response");

                        None
                    }
                },
                Err(err) => {
                    warn!(err = ?Report::new(err), "Failed to respond");

                    None
                }
            },
            None => None,
        };

        let allow_custom_skins = match guild {
            Some(guild_id) => {
                Context::guild_config()
                    .peek(guild_id, |config| config.allow_custom_skins.unwrap_or(true))
                    .await
            }
            None => true,
        };

        let skin = settings.skin(allow_custom_skins);

        debug!(score_id, discord = owner.get(), "Commissioning render");

        let render_fut = Context::ordr()
            .client()
            .render_with_replay_file(&replay, RENDERER_NAME, &skin.skin)
            .options(settings.options());

        let render = match render_fut.await {
            Ok(render) => render,
            Err(err) => {
                let content = match err {
                    OrdrError::Response {
                        error:
                            OrdrApiError {
                                code: Some(code), ..
                            },
                        ..
                    } => format!("Error code {int} from o!rdr: {code}", int = code.to_u8()),
                    err => {
                        error!(?err, "Failed to commission render");

                        ORDR_ISSUE.to_owned()
                    }
                };

                let embed = EmbedBuilder::new().color_red().description(content);
                let builder = MessageBuilder::new().embed(embed);

                if let Some(update_fut) = msg.update(builder, permissions) {
                    let _ = update_fut.await;
                }

                return;
            }
        };

        let ongoing_fut = OngoingRender::new(
            render.render_id,
            OwnedCommandOrigin::Message {
                msg: orig.0,
                channel: orig.1,
                permissions,
            },
            ProgressResponse::new(response, permissions, true),
            status,
            Some(score_id),
            owner,
        );

        ongoing_fut.await.await_render_url().await;
    }
}

impl IActiveMessage for SingleScorePagination {
    async fn build_page(&mut self) -> Result<BuildPage> {
        let content = match self.content {
            SingleScoreContent::SameForAll(ref content) => content.as_str().into(),
            SingleScoreContent::OnlyForIndex { idx, ref content } if idx == self.pages.index() => {
                content.as_str().into()
            }
            SingleScoreContent::OnlyForIndex { .. } | SingleScoreContent::None => Box::default(),
        };

        self.async_build_page(content, MarkIndex::Skip).await
    }

    fn build_components(&self) -> Vec<Component> {
        let mut all_components = if self.settings.buttons.pagination {
            self.pages.components()
        } else {
            Vec::new()
        };

        let score = self.scores[self.pages.index()]
            .try_get()
            .expect("score data not yet expanded");

        if score.miss_analyzer.is_some() || score.replay_score_id.is_some() {
            let mut components = Vec::with_capacity(2);

            if score.miss_analyzer.is_some() {
                components.push(Component::Button(Button {
                    custom_id: Some("miss_analyzer".to_owned()),
                    disabled: false,
                    emoji: Some(Emote::Miss.reaction_type()),
                    label: Some("Miss analyzer".to_owned()),
                    style: ButtonStyle::Primary,
                    url: None,
                    sku_id: None,
                }));
            }

            if score.replay_score_id.is_some() {
                components.push(Component::Button(Button {
                    custom_id: Some("render".to_owned()),
                    disabled: false,
                    emoji: Some(EmojiReactionType::Unicode {
                        name: "🎥".to_owned(),
                    }),
                    label: Some("Render".to_owned()),
                    style: ButtonStyle::Primary,
                    url: None,
                    sku_id: None,
                }));
            }

            all_components.push(Component::ActionRow(ActionRow { components }));
        }

        all_components
    }

    async fn handle_component(&mut self, component: &mut InteractionComponent) -> ComponentResult {
        let user_id = match component.user_id() {
            Ok(user_id) => user_id,
            Err(err) => return ComponentResult::Err(err),
        };

        // Render and miss analyzer buttons are allowed to be pressed by
        // anyone - not just the initial owner

        match component.data.custom_id.as_str() {
            "render" => self.handle_render_button(component).await,
            "miss_analyzer" => self.handle_miss_analyzer_button(component).await,
            _ => {
                if user_id != self.msg_owner {
                    return ComponentResult::Ignore;
                }

                handle_pagination_component(component, self.msg_owner, false, &mut self.pages).await
            }
        }
    }

    async fn handle_modal(&mut self, modal: &mut InteractionModal) -> Result<()> {
        handle_pagination_modal(modal, self.msg_owner, false, &mut self.pages).await
    }

    fn until_timeout(&self) -> Option<Duration> {
        (!self.build_components().is_empty()).then_some(Duration::from_secs(60))
    }
}

pub enum SingleScoreContent {
    SameForAll(String),
    OnlyForIndex { idx: usize, content: String },
    None,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum MarkIndex {
    /// Don't mark anything
    Skip,
    /// Mark the given index
    Some(usize),
    /// Don't mark anything but denote that this value came from the builder
    None,
}

fn apply_settings(
    settings: &ScoreEmbedSettings,
    data: &ScoreEmbedData,
    score_data: ScoreData,
    mark_idx: MarkIndex,
) -> EmbedBuilder {
    const SEP_NAME: &str = "\t";
    const SEP_VALUE: &str = " • ";

    let map_attrs = data.map.attributes().mods(data.score.mods.clone()).build();

    let mut field_name = String::new();
    let mut field_value = String::new();
    let mut footer_text = String::new();

    let mut writer = &mut field_name;

    let hide_ratio = || data.score.mode != GameMode::Mania && mark_idx == MarkIndex::Skip;

    let hide_mapper_status = || {
        matches!(
            data.map.status(),
            RankStatus::Ranked | RankStatus::Loved | RankStatus::Approved | RankStatus::Qualified
        ) && data.map.ranked_date().is_some()
            && settings
                .values
                .iter()
                .any(|value| ValueKind::from_setting(value) == ValueKind::MapRankedDate)
    };

    let hide_ranked_date = || data.map.ranked_date().is_none();

    for (i, curr) in settings.values.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|i| settings.values.get(i));
        let next = settings.values.get(i + 1);

        match (prev.map(|p| &p.inner), &curr.inner, next.map(|n| &n.inner)) {
            (Some(Value::Grade), Value::Mods, _) if prev.is_some_and(|p| p.y == curr.y) => {
                // Simple whitespace as separator for this case
                writer.push(' ');

                if mark_idx == MarkIndex::Some(i) {
                    writer.push_str("__");
                }

                write_value(curr, data, &map_attrs, score_data, writer);

                if mark_idx == MarkIndex::Some(i) {
                    writer.push_str("__");
                }
            }
            (
                Some(Value::Ar | Value::Cs | Value::Hp | Value::Od),
                Value::Ar | Value::Cs | Value::Hp | Value::Od,
                Some(Value::Ar | Value::Cs | Value::Hp | Value::Od),
            ) if prev.is_some_and(|p| p.y == curr.y) && next.is_some_and(|n| curr.y == n.y) => {
                // We're already within "`" boundaries

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                let fmt = match curr.inner {
                    Value::Ar => MapAttribute::AR.fmt(data, &map_attrs),
                    Value::Cs => MapAttribute::CS.fmt(data, &map_attrs),
                    Value::Hp => MapAttribute::HP.fmt(data, &map_attrs),
                    Value::Od => MapAttribute::OD.fmt(data, &map_attrs),
                    _ => unreachable!(),
                };

                let _ = write!(writer, "{fmt}");

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                writer.push(' ');
            }
            (
                Some(Value::Ar | Value::Cs | Value::Hp | Value::Od),
                Value::Ar | Value::Cs | Value::Hp | Value::Od,
                _,
            ) if prev.is_some_and(|p| p.y == curr.y) => {
                // We're the last of the "`" boundary

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                let fmt = match curr.inner {
                    Value::Ar => MapAttribute::AR.fmt(data, &map_attrs),
                    Value::Cs => MapAttribute::CS.fmt(data, &map_attrs),
                    Value::Hp => MapAttribute::HP.fmt(data, &map_attrs),
                    Value::Od => MapAttribute::OD.fmt(data, &map_attrs),
                    _ => unreachable!(),
                };

                let _ = write!(writer, "{fmt}");

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                if curr.y < SettingValue::FOOTER_Y {
                    writer.push('`');
                }
            }
            (
                _,
                Value::Ar | Value::Cs | Value::Hp | Value::Od,
                Some(Value::Ar | Value::Cs | Value::Hp | Value::Od),
            ) if next.is_some_and(|n| curr.y == n.y) => {
                // We're the first of the "`" boundary

                if prev.is_some_and(|p| p.y == curr.y) {
                    let sep = if curr.y == SettingValue::NAME_Y {
                        SEP_NAME
                    } else {
                        SEP_VALUE
                    };
                    writer.push_str(sep);
                } else if curr.y == SettingValue::FOOTER_Y {
                    writer = &mut footer_text;
                } else if prev.is_some_and(|p| p.y == SettingValue::NAME_Y) {
                    writer = &mut field_value;
                } else {
                    writer.push('\n');
                }

                if curr.y < SettingValue::FOOTER_Y {
                    writer.push('`');
                }

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                let fmt = match curr.inner {
                    Value::Ar => MapAttribute::AR.fmt(data, &map_attrs),
                    Value::Cs => MapAttribute::CS.fmt(data, &map_attrs),
                    Value::Hp => MapAttribute::HP.fmt(data, &map_attrs),
                    Value::Od => MapAttribute::OD.fmt(data, &map_attrs),
                    _ => unreachable!(),
                };

                let _ = write!(writer, "{fmt}");

                if mark_idx == MarkIndex::Some(i) {
                    writer.push('*');
                }

                writer.push(' ');
            }
            (_, Value::Ratio, _) if hide_ratio() => {}
            (_, Value::MapRankedDate, _) if hide_ranked_date() => {}
            _ => {
                let mut value = Cow::Borrowed(curr);

                if matches!(&curr.inner, Value::Mapper(mapper) if mapper.with_status)
                    && hide_mapper_status()
                {
                    value = Cow::Owned(SettingValue {
                        inner: Value::Mapper(MapperValue { with_status: false }),
                        y: curr.y,
                    });
                }

                let curr = value.as_ref();

                let need_sep = settings.values[..i]
                    .iter()
                    .rev()
                    .take_while(|prev| prev.y == curr.y)
                    .any(|prev| {
                        !((prev.inner == Value::Ratio && hide_ratio())
                            || (prev.inner == Value::MapRankedDate && hide_ranked_date()))
                    });

                if need_sep {
                    let sep = if curr.y == SettingValue::NAME_Y {
                        SEP_NAME
                    } else {
                        SEP_VALUE
                    };
                    writer.push_str(sep);
                } else if curr.y == SettingValue::FOOTER_Y {
                    writer = &mut footer_text;
                } else if let Some(prev) = prev {
                    if prev.y == SettingValue::NAME_Y {
                        writer = &mut field_value;
                    } else {
                        writer.push('\n');
                    }
                }

                let mark = if value.y == SettingValue::FOOTER_Y {
                    "*"
                } else {
                    "__"
                };

                if mark_idx == MarkIndex::Some(i) {
                    writer.push_str(mark);
                }

                write_value(&value, data, &map_attrs, score_data, writer);

                if mark_idx == MarkIndex::Some(i) {
                    writer.push_str(mark);
                }
            }
        }
    }

    let fields = fields![field_name, field_value, false];

    let mut title = String::with_capacity(32);

    if settings.show_artist {
        let _ = write!(title, "{} - ", data.map.artist().cow_escape_markdown());
    }

    let _ = write!(
        title,
        "{} [{}]",
        data.map.title().cow_escape_markdown(),
        data.map.version().cow_escape_markdown()
    );

    if settings.show_sr_in_title {
        let _ = write!(title, " [{}★]", round(data.stars));
    }

    let mut builder = EmbedBuilder::new().fields(fields).title(title);

    match settings.image {
        SettingsImage::Thumbnail => builder = builder.thumbnail(data.map.thumbnail()),
        SettingsImage::Image => builder = builder.image(data.map.cover()),
        SettingsImage::ImageWithStrains => {
            builder = builder.image(attachment(SingleScorePagination::IMAGE_NAME));
        }
        SettingsImage::Hide => {}
    }

    if !footer_text.is_empty() {
        let emote = Emote::from(data.score.mode).url();
        let footer = FooterBuilder::new(footer_text).icon_url(emote);
        builder = builder.footer(footer);
    }

    builder
}

const DAY: Duration = Duration::from_secs(60 * 60 * 24);

fn write_value(
    value: &SettingValue,
    data: &ScoreEmbedData,
    map_attrs: &BeatmapAttributes,
    score_data: ScoreData,
    writer: &mut String,
) {
    match &value.inner {
        Value::Grade => {
            let _ = if value.y == SettingValue::NAME_Y {
                write!(
                    writer,
                    "{}",
                    GradeFormatter::new(data.score.grade, None, false),
                )
            } else if value.y == SettingValue::FOOTER_Y {
                write!(writer, "{:?}", data.score.grade)
            } else {
                write!(
                    writer,
                    "{}",
                    GradeFormatter::new(data.score.grade, Some(data.score.score_id), false),
                )
            };

            // The completion is very hard to calculate for `Catch` because
            // `n_objects` is not correct due to juicestreams so we won't
            // show it for that mode.
            let is_fail = data.score.grade == Grade::F && data.score.mode != GameMode::Catch;

            if is_fail {
                let n_objects = data.map.n_objects();

                let completion = if n_objects != 0 {
                    100 * data.score.total_hits() / n_objects
                } else {
                    100
                };

                let _ = write!(writer, "@{completion}%");
            }
        }
        Value::Mods => {
            let _ = write!(
                writer,
                "+{}",
                ModsFormatter::new(&data.score.mods, data.score.is_legacy)
            );
        }
        Value::Score => {
            let _ = write!(writer, "{}", ScoreFormatter::new(&data.score, score_data));
        }
        Value::Accuracy => {
            let _ = write!(writer, "{}%", round(data.score.accuracy));
        }
        Value::ScoreDate => {
            let score_date = data.score.ended_at;

            if value.y == SettingValue::FOOTER_Y {
                writer.push_str("Played ");

                if OffsetDateTime::now_utc() < score_date + DAY {
                    let _ = write!(writer, "{}", HowLongAgoText::new(&score_date));
                } else {
                    writer.push_str(&score_date.format(&SHORT_NAIVE_DATETIME_FORMAT).unwrap());
                    writer.push_str(" UTC");
                }
            } else {
                let _ = write!(writer, "{}", HowLongAgoDynamic::new(&score_date));
            }
        }
        Value::Pp(pp) => {
            let bold = if value.y < SettingValue::FOOTER_Y {
                "**"
            } else {
                ""
            };
            let tilde = if value.y < SettingValue::FOOTER_Y {
                "~~"
            } else {
                ""
            };

            let _ = write!(writer, "{bold}{:.2}", data.score.pp);

            let _ = match (pp.max, data.if_fc_pp.filter(|_| pp.if_fc), pp.max_if_fc) {
                (true, Some(if_fc_pp), _) => {
                    write!(
                        writer,
                        "{bold}/{max:.2}PP {tilde}({if_fc_pp:.2}pp){tilde}",
                        max = data.max_pp.max(data.score.pp)
                    )
                }
                (true, None, _) | (false, None, true) => {
                    write!(writer, "{bold}/{:.2}PP", data.max_pp.max(data.score.pp))
                }
                (false, Some(if_fc_pp), _) => {
                    write!(writer, "pp{bold} {tilde}({if_fc_pp:.2}pp){tilde}")
                }
                (false, None, false) => write!(writer, "pp{bold}"),
            };
        }
        Value::Combo(combo) => {
            if value.y < SettingValue::FOOTER_Y {
                writer.push_str("**");
            }

            let _ = write!(writer, "{}x", data.score.max_combo);

            if value.y < SettingValue::FOOTER_Y {
                writer.push_str("**");
            }

            if combo.max {
                let _ = write!(writer, "/{}x", data.max_combo);
            }
        }
        Value::Hitresults(hitresults) => {
            let _ = match hitresults {
                HitresultsValue::Full => write!(
                    writer,
                    "{}",
                    HitResultFormatter::new(data.score.mode, &data.score.statistics)
                ),
                HitresultsValue::OnlyMisses if value.y < SettingValue::FOOTER_Y => {
                    write!(writer, "{}{}", data.score.statistics.miss, Emote::Miss)
                }
                HitresultsValue::OnlyMisses => {
                    write!(writer, "{} miss", data.score.statistics.miss)
                }
            };
        }
        Value::Ratio => {
            let mut ratio = data.score.statistics.perfect as f32;

            let against: u8 = if data.score.statistics.great > 0 {
                ratio /= data.score.statistics.great as f32;

                1
            } else {
                0
            };

            let _ = write!(writer, "{ratio:.2}:{against}");
        }
        Value::ScoreId => {
            let url = |writer: &mut String| match score_data {
                ScoreData::Stable => write!(
                    writer,
                    "{OSU_BASE}scores/{}/{}",
                    data.score.mode, data.score.score_id
                ),
                _ => write!(writer, "{OSU_BASE}scores/{}", data.score.score_id),
            };

            if value.y == SettingValue::NAME_Y {
                let _ = url(writer);
            } else if value.y == SettingValue::FOOTER_Y {
                let _ = write!(writer, "Score id {}", data.score.score_id);
            } else {
                writer.push_str("[Link to score](");
                let _ = url(writer);
                writer.push(')');
            }
        }
        Value::Stars => {
            let _ = write!(writer, "{}★", round(data.stars));
        }
        Value::Length => {
            let clock_rate = map_attrs.clock_rate as f32;
            let seconds_drain = (data.map.seconds_drain() as f32 / clock_rate) as u32;

            if value.y < SettingValue::FOOTER_Y {
                writer.push('`');
            }

            let _ = write!(writer, "{}", SecToMinSec::new(seconds_drain).pad_secs());

            if value.y < SettingValue::FOOTER_Y {
                writer.push('`');
            }
        }
        Value::Ar | Value::Cs | Value::Hp | Value::Od => {
            if value.y < SettingValue::FOOTER_Y {
                writer.push('`');
            }

            let fmt = match &value.inner {
                Value::Ar => MapAttribute::AR.fmt(data, map_attrs),
                Value::Cs => MapAttribute::CS.fmt(data, map_attrs),
                Value::Hp => MapAttribute::HP.fmt(data, map_attrs),
                Value::Od => MapAttribute::OD.fmt(data, map_attrs),
                _ => unreachable!(),
            };

            let _ = write!(writer, "{fmt}");

            if value.y < SettingValue::FOOTER_Y {
                writer.push('`');
            }
        }
        Value::Bpm(emote_text) => {
            let clock_rate = map_attrs.clock_rate as f32;
            let bpm = round(data.map.bpm() * clock_rate);

            if value.y < SettingValue::FOOTER_Y {
                writer.push_str("**");
            }

            let _ = match emote_text {
                EmoteTextValue::Emote if value.y < SettingValue::FOOTER_Y => {
                    write!(writer, "{} {bpm}", Emote::Bpm)
                }
                EmoteTextValue::Text | EmoteTextValue::Emote => write!(writer, "{bpm} BPM"),
            };

            if value.y < SettingValue::FOOTER_Y {
                writer.push_str("**");
            }
        }
        Value::CountObjects(emote_text) => {
            let n = data.map.n_objects();

            let _ = match emote_text {
                EmoteTextValue::Emote if value.y < SettingValue::FOOTER_Y => {
                    write!(writer, "{} {n}", Emote::CountObjects)
                }
                EmoteTextValue::Text | EmoteTextValue::Emote => {
                    write!(
                        writer,
                        "{n} object{plural}",
                        plural = if n == 1 { "" } else { "s" }
                    )
                }
            };
        }
        Value::CountSliders(emote_text) => {
            let n = data.map.n_sliders();

            let _ = match emote_text {
                EmoteTextValue::Emote if value.y < SettingValue::FOOTER_Y => {
                    write!(writer, "{} {n}", Emote::CountSliders)
                }
                EmoteTextValue::Text | EmoteTextValue::Emote => {
                    write!(
                        writer,
                        "{n} slider{plural}",
                        plural = if n == 1 { "" } else { "s" }
                    )
                }
            };
        }
        Value::CountSpinners(emote_text) => {
            let n = data.map.n_spinners();

            let _ = match emote_text {
                EmoteTextValue::Emote if value.y < SettingValue::FOOTER_Y => {
                    write!(writer, "{} {n}", Emote::CountSpinners)
                }
                EmoteTextValue::Text | EmoteTextValue::Emote => {
                    write!(
                        writer,
                        "{n} spinner{plural}",
                        plural = if n == 1 { "" } else { "s" }
                    )
                }
            };
        }
        Value::MapRankedDate => {
            if let Some(ranked_date) = data.map.ranked_date() {
                let _ = write!(writer, "{:?} ", data.map.status());

                if OffsetDateTime::now_utc() < ranked_date + DAY {
                    let _ = if value.y == SettingValue::FOOTER_Y {
                        write!(writer, "{}", HowLongAgoText::new(&ranked_date))
                    } else {
                        write!(writer, "{}", HowLongAgoDynamic::new(&ranked_date))
                    };
                } else if value.y == SettingValue::FOOTER_Y {
                    writer.push_str(&ranked_date.format(&SHORT_NAIVE_DATETIME_FORMAT).unwrap());
                    writer.push_str(" UTC");
                } else {
                    let _ = write!(writer, "<t:{}:f>", ranked_date.unix_timestamp());
                }
            }
        }
        Value::Mapper(mapper) => {
            let creator = data.map.creator();

            let _ = if mapper.with_status {
                write!(writer, "{:?} mapset by {creator}", data.map.status())
            } else {
                write!(writer, "Mapset by {creator}")
            };
        }
    }
}

struct MapAttributeFormatter<'a> {
    map_attr: MapAttribute,
    data: &'a ScoreEmbedData,
    value: f64,
}

impl<'a> MapAttributeFormatter<'a> {
    fn new(data: &'a ScoreEmbedData, map_attr: MapAttribute, value: f64) -> Self {
        Self {
            map_attr,
            data,
            value,
        }
    }
}

impl Display for MapAttributeFormatter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let map_attr_str = match self.map_attr {
            MapAttribute::AR => "AR",
            MapAttribute::CS => "CS",
            MapAttribute::HP => "HP",
            MapAttribute::OD => "OD",
        };

        write!(f, "{map_attr_str}: {}", round(self.value as f32))?;

        let mods = &self.data.score.mods;

        if !self.map_attr.is_difficulty_adjusted(mods) {
            return Ok(());
        }

        let alt_mods: GameMods = self
            .data
            .score
            .mods
            .iter()
            .filter_map(|m| match m {
                GameMod::DifficultyAdjustOsu(_)
                | GameMod::DifficultyAdjustTaiko(_)
                | GameMod::DifficultyAdjustCatch(_)
                | GameMod::DifficultyAdjustMania(_) => None,
                _ => Some(m.to_owned()),
            })
            .collect();

        let map_attrs = self.data.map.attributes().mods(alt_mods).build();
        let alt_value = self.map_attr.get_value(&map_attrs);

        let symbol = match self.value.partial_cmp(&alt_value) {
            Some(Ordering::Less) => "⬇",
            Some(Ordering::Greater) => "⬆",
            None | Some(Ordering::Equal) => return Ok(()),
        };

        f.write_str(symbol)
    }
}

#[derive(Copy, Clone)]
enum MapAttribute {
    AR,
    CS,
    HP,
    OD,
}

impl MapAttribute {
    fn fmt<'a>(
        self,
        data: &'a ScoreEmbedData,
        attrs: &BeatmapAttributes,
    ) -> MapAttributeFormatter<'a> {
        MapAttributeFormatter::new(data, self, self.get_value(attrs))
    }

    fn get_value(self, attrs: &BeatmapAttributes) -> f64 {
        match self {
            MapAttribute::AR => attrs.ar,
            MapAttribute::CS => attrs.cs,
            MapAttribute::HP => attrs.hp,
            MapAttribute::OD => attrs.od,
        }
    }
}

macro_rules! impl_is_difficulty_adjusted {
    ( $(
        $mod_variant:ident {
            $( $self_variant:ident: $field:ident, )+
        },
    )+ ) => {
        impl MapAttribute {
            fn is_difficulty_adjusted(self, mods: &GameMods) -> bool {
                mods.iter().any(|m| {
                    match m {
                        $(
                            GameMod::$mod_variant(m) => match self {
                                $( Self::$self_variant => m.$field.is_some(), )*
                                #[allow(unreachable_patterns)]
                                _ => false,
                            },
                        )*
                        _ => false,
                    }
                })
            }
        }
    };
}

impl_is_difficulty_adjusted! {
    DifficultyAdjustOsu {
        AR: approach_rate,
        CS: circle_size,
        HP: drain_rate,
        OD: overall_difficulty,
    },
    DifficultyAdjustTaiko {
        HP: drain_rate,
        OD: overall_difficulty,
    },
    DifficultyAdjustCatch {
        AR: approach_rate,
        CS: circle_size,
        HP: drain_rate,
        OD: overall_difficulty,
    },
    DifficultyAdjustMania {
        HP: drain_rate,
        OD: overall_difficulty,
    },
}
