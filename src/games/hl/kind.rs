use std::{fmt::Write, mem, sync::Arc};

use eyre::Report;
use image::{png::PngEncoder, ColorType};
use rosu_v2::prelude::GameMode;
use tokio::sync::oneshot::{self, Receiver};
use twilight_model::channel::embed::EmbedField;

use crate::{
    core::{Context, CONFIG},
    error::InvalidGameState,
    games::hl::score_pp::ScorePp,
    util::{
        builder::{EmbedBuilder, MessageBuilder},
        numbers::round,
        ChannelExt,
    },
    BotResult,
};

use super::{HlGuess, HlVersion, H, W};

pub(super) enum GameStateKind {
    ScorePp {
        mode: GameMode,
        previous: ScorePp,
        next: ScorePp,
    },
}

impl GameStateKind {
    pub(super) fn check_guess(&self, guess: HlGuess) -> bool {
        match self {
            Self::ScorePp { previous, next, .. } => match guess {
                HlGuess::Higher => next.pp >= previous.pp,
                HlGuess::Lower => next.pp <= previous.pp,
            },
        }
    }

    pub async fn restart(self, ctx: &Context) -> BotResult<(Self, Receiver<String>)> {
        match self {
            Self::ScorePp { mode, .. } => Self::score_pp(ctx, mode).await,
        }
    }

    pub async fn next(
        &mut self,
        ctx: Arc<Context>,
        curr_score: u32,
    ) -> BotResult<Receiver<String>> {
        let rx = match self {
            Self::ScorePp {
                mode,
                previous,
                next,
            } => {
                let mode = *mode;
                mem::swap(previous, next);

                *next = ScorePp::random(&ctx, mode, previous.pp, curr_score).await?;

                while previous == next {
                    *next = ScorePp::random(&ctx, mode, previous.pp, curr_score).await?;
                }

                debug!("{}pp vs {}pp", previous.pp, next.pp);

                let pfp1 = mem::take(&mut previous.avatar);

                // Clone this since it's needed in the next round
                let pfp2 = next.avatar.clone();

                let mapset1 = previous.mapset_id;
                let mapset2 = next.mapset_id;

                let (tx, rx) = oneshot::channel();

                // Create the image in the background so it's available when needed later
                tokio::spawn(async move {
                    let url = match ScorePp::image(&ctx, &pfp1, &pfp2, mapset1, mapset2).await {
                        Ok(url) => url,
                        Err(err) => {
                            let report = Report::new(err).wrap_err("failed to create image");
                            warn!("{report:?}");

                            String::new()
                        }
                    };

                    let _ = tx.send(url);
                });

                rx
            }
        };

        Ok(rx)
    }

    pub async fn score_pp(ctx: &Context, mode: GameMode) -> BotResult<(Self, Receiver<String>)> {
        let (previous, mut next) = tokio::try_join!(
            ScorePp::random(ctx, mode, 0.0, 0),
            ScorePp::random(ctx, mode, 0.0, 0)
        )?;

        while next == previous {
            next = ScorePp::random(ctx, mode, 0.0, 0).await?;
        }

        debug!("{}pp vs {}pp", previous.pp, next.pp);

        let (tx, rx) = oneshot::channel();

        let pfp1 = &previous.avatar;
        let mapset1 = previous.mapset_id;

        let pfp2 = &next.avatar;
        let mapset2 = next.mapset_id;

        let url = match ScorePp::image(ctx, pfp1, pfp2, mapset1, mapset2).await {
            Ok(url) => url,
            Err(err) => {
                let report = Report::new(err).wrap_err("failed to create image");
                warn!("{report:?}");

                String::new()
            }
        };

        let _ = tx.send(url);

        let inner = Self::ScorePp {
            mode,
            previous,
            next,
        };

        Ok((inner, rx))
    }

    pub fn to_embed(&self, image: String) -> EmbedBuilder {
        let mut title = "Higher or Lower: ".to_owned();

        let fields = match self {
            Self::ScorePp {
                mode,
                previous,
                next,
            } => {
                title.push_str("Score PP");

                match mode {
                    GameMode::STD => {}
                    GameMode::TKO => title.push_str(" (taiko)"),
                    GameMode::CTB => title.push_str(" (ctb)"),
                    GameMode::MNA => title.push_str(" (mania)"),
                }

                vec![
                    EmbedField {
                        inline: false,
                        name: format!("__Previous:__ {}", previous.player_string),
                        value: previous.play_string(true),
                    },
                    EmbedField {
                        inline: false,
                        name: format!("__Next:__ {}", next.player_string),
                        value: next.play_string(false),
                    },
                ]
            }
        };

        EmbedBuilder::new().title(title).fields(fields).image(image)
    }

    pub fn reveal(&self, field: &mut EmbedField) {
        match self {
            Self::ScorePp { next, .. } => {
                field.value.truncate(field.value.len() - 7);
                let _ = write!(field.value, "__{}pp__**", round(next.pp));
            }
        }
    }

    pub fn version(&self) -> HlVersion {
        match self {
            Self::ScorePp { .. } => HlVersion::ScorePp,
        }
    }

    pub async fn upload_image(ctx: &Context, img: &[u8], content: String) -> BotResult<String> {
        // Encode the combined images
        let mut png_bytes: Vec<u8> = Vec::with_capacity((W * H * 4) as usize);
        let png_encoder = PngEncoder::new(&mut png_bytes);
        png_encoder.encode(img, W, H, ColorType::Rgba8)?;

        // Send image into discord channel
        let builder = MessageBuilder::new()
            .attachment("higherlower.png", png_bytes)
            .content(content);

        let mut message = CONFIG
            .get()
            .unwrap()
            .hl_channel
            .create_message(ctx, &builder)
            .await?
            .model()
            .await?;

        // Return the url to the message's image
        let attachment = message
            .attachments
            .pop()
            .ok_or(InvalidGameState::MissingAttachment)?;

        Ok(attachment.url)
    }
}