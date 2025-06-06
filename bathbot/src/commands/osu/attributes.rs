use bathbot_macros::SlashCommand;
use bathbot_util::{
    MessageBuilder, matcher,
    osu::{AttributeKind, ModSelection},
};
use eyre::Result;
use rosu_v2::{model::mods::GameModsIntermode, prelude::GameMode};
use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::{
    embeds::{AttributesEmbed, EmbedData},
    util::{InteractionCommandExt, interaction::InteractionCommand},
};

#[derive(CommandModel, CreateCommand, SlashCommand)]
#[command(
    name = "attributes",
    desc = "Check how mods influence the AR, OD, HP, or CS attributes"
)]
#[flags(SKIP_DEFER)]
pub enum Attributes {
    #[command(name = "ar")]
    Ar(AttributesAr),
    #[command(name = "cs")]
    Cs(AttributesCs),
    #[command(name = "hp")]
    Hp(AttributesHp),
    #[command(name = "od")]
    Od(AttributesOd),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "ar",
    desc = "Check how mods influence the approach rate attribute"
)]
pub struct AttributesAr {
    #[command(rename = "value", min_value = -15.0, max_value = 13.0, desc = "Specify an AR value")]
    number: f32,
    #[command(
        desc = "Specify mods e.g. hdhr or nm",
        help = "Specify mods either directly or through the explicit `+mod!` / `+mod` syntax, \
        e.g. `hdhr` or `+hdhr!`"
    )]
    mods: String,
    #[command(desc = "Specify a custom clock rate that overwrites mods")]
    clock_rate: Option<f32>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "cs",
    desc = "Check how mods influence the circle size attribute"
)]
pub struct AttributesCs {
    #[command(
        rename = "value",
        min_value = 0.0,
        max_value = 20.0,
        desc = "Specify a CS value"
    )]
    number: f32,
    #[command(
        desc = "Specify mods e.g. hdhr or nm",
        help = "Specify mods either directly or through the explicit `+mod!` / `+mod` syntax, \
        e.g. `hdhr` or `+hdhr!`"
    )]
    mods: String,
    #[command(desc = "Specify a custom clock rate that overwrites mods")]
    clock_rate: Option<f32>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "hp",
    desc = "Check how mods influence the drain rate attribute"
)]
pub struct AttributesHp {
    #[command(
        rename = "value",
        min_value = 0.0,
        max_value = 20.0,
        desc = "Specify an HP value"
    )]
    number: f32,
    #[command(
        desc = "Specify mods e.g. hdhr or nm",
        help = "Specify mods either directly or through the explicit `+mod!` / `+mod` syntax, \
        e.g. `hdhr` or `+hdhr!`"
    )]
    mods: String,
    #[command(desc = "Specify a custom clock rate that overwrites mods")]
    clock_rate: Option<f32>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "od",
    desc = "Check how mods influence the overall difficulty attribute"
)]
pub struct AttributesOd {
    #[command(rename = "value", min_value = -13.33, max_value = 13.33, desc = "Specify an OD value")]
    number: f32,
    #[command(
        desc = "Specify mods e.g. hdhr or nm",
        help = "Specify mods either directly or through the explicit `+mod!` / `+mod` syntax, \
        e.g. `hdhr` or `+hdhr!`"
    )]
    mods: String,
    #[command(desc = "Specify a custom clock rate that overwrites mods")]
    clock_rate: Option<f32>,
}

async fn slash_attributes(mut command: InteractionCommand) -> Result<()> {
    let attrs = Attributes::from_interaction(command.input_data())?;

    let (kind, value, mods, clock_rate) = match attrs {
        Attributes::Ar(args) => (AttributeKind::Ar, args.number, args.mods, args.clock_rate),
        Attributes::Cs(args) => (AttributeKind::Cs, args.number, args.mods, args.clock_rate),
        Attributes::Hp(args) => (AttributeKind::Hp, args.number, args.mods, args.clock_rate),
        Attributes::Od(args) => (AttributeKind::Od, args.number, args.mods, args.clock_rate),
    };

    let mods = if let Some(mods) = GameModsIntermode::try_from_acronyms(&mods) {
        mods
    } else {
        match matcher::get_mods(&mods) {
            Some(ModSelection::Include(mods) | ModSelection::Exact(mods)) => mods,
            None => {
                let content =
                    "Failed to parse mods. Be sure to specify a valid mod combination e.g. `hrdt`.";
                command.error_callback(content).await?;

                return Ok(());
            }
            Some(ModSelection::Exclude { .. }) => {
                let content = "Excluding mods does not work for this command";
                command.error_callback(content).await?;

                return Ok(());
            }
        }
    };

    let valid_mods = [
        GameMode::Osu,
        GameMode::Taiko,
        GameMode::Catch,
        GameMode::Mania,
    ]
    .into_iter()
    .any(|mode| mods.clone().with_mode(mode).is_valid());

    if !valid_mods {
        let content = "Looks like either some of these mods are incompatible with each other \
            or those mods don't fit to any gamemode.";
        command.error_callback(content).await?;

        return Ok(());
    }

    let embed = AttributesEmbed::new(kind, value, mods, clock_rate).build();
    let builder = MessageBuilder::new().embed(embed);
    command.callback(builder, false).await?;

    Ok(())
}
