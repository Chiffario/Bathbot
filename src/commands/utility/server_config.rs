use std::sync::Arc;

use twilight_cache_inmemory::model::CachedGuild;
use twilight_model::{
    application::{
        command::CommandOptionChoice,
        interaction::{application_command::CommandOptionValue, ApplicationCommand},
    },
    id::{marker::GuildMarker, Id},
    util::ImageHash,
};

use crate::{
    commands::{osu::ProfileSize, MyCommand, MyCommandOption},
    database::GuildConfig,
    embeds::{EmbedData, ServerConfigEmbed},
    util::{
        constants::{common_literals::PROFILE, GENERAL_ISSUE},
        MessageExt,
    },
    BotResult, Context, Error,
};

use super::AuthorityCommandKind;

enum ServerConfigCommandKind {
    Args(ServerConfigArgs),
    Auth(AuthorityCommandKind),
}

struct ServerConfigArgs {
    embeds_maximized: Option<bool>,
    profile_size: Option<ProfileSize>,
    show_retries: Option<bool>,
    togglesongs: Option<bool>,
}

impl ServerConfigArgs {
    fn any(&self) -> bool {
        let ServerConfigArgs {
            embeds_maximized,
            profile_size,
            show_retries,
            togglesongs,
        } = self;

        embeds_maximized.is_some()
            || profile_size.is_some()
            || show_retries.is_some()
            || togglesongs.is_some()
    }
}

impl ServerConfigCommandKind {
    fn slash(command: &ApplicationCommand) -> BotResult<Self> {
        command
            .data
            .options
            .first()
            .and_then(|option| match &option.value {
                CommandOptionValue::SubCommand(options) if option.name == "edit" => {
                    let mut embeds_maximized = None;
                    let mut profile_size = None;
                    let mut show_retries = None;
                    let mut togglesongs = None;

                    for option in options {
                        match &option.value {
                            CommandOptionValue::String(value) => match option.name.as_str() {
                                "embeds" => embeds_maximized = Some(value == "maximized"),
                                "profile" => match value.as_str() {
                                    "compact" => profile_size = Some(ProfileSize::Compact),
                                    "medium" => profile_size = Some(ProfileSize::Medium),
                                    "full" => profile_size = Some(ProfileSize::Full),
                                    _ => return None,
                                },
                                "retries" => show_retries = Some(value == "show"),
                                "song_commands" => togglesongs = Some(value == "enable"),
                                _ => return None,
                            },
                            _ => return None,
                        }
                    }

                    let args = ServerConfigArgs {
                        embeds_maximized,
                        profile_size,
                        show_retries,
                        togglesongs,
                    };

                    Some(Self::Args(args))
                }
                CommandOptionValue::SubCommandGroup(options) if option.name == "authorities" => {
                    let option = options.first()?;

                    match &option.value {
                        CommandOptionValue::SubCommand(options) => match option.name.as_str() {
                            "add" => match options.first()?.value {
                                CommandOptionValue::Role(id) => {
                                    Some(Self::Auth(AuthorityCommandKind::Add(id.get())))
                                }
                                _ => None,
                            },
                            "list" => Some(Self::Auth(AuthorityCommandKind::List)),
                            "remove" => match options.first()?.value {
                                CommandOptionValue::Role(id) => {
                                    Some(Self::Auth(AuthorityCommandKind::Remove(id.get())))
                                }
                                _ => None,
                            },
                            _ => None,
                        },
                        _ => None,
                    }
                }
                _ => None,
            })
            .ok_or(Error::InvalidCommandOptions)
    }
}

pub struct GuildData {
    pub icon: Option<ImageHash>,
    pub id: Id<GuildMarker>,
    pub name: String,
}

impl From<&CachedGuild> for GuildData {
    fn from(guild: &CachedGuild) -> Self {
        Self {
            icon: guild.icon().map(ImageHash::to_owned),
            id: guild.id(),
            name: guild.name().to_owned(),
        }
    }
}

pub async fn slash_serverconfig(ctx: Arc<Context>, command: ApplicationCommand) -> BotResult<()> {
    let guild_id = command.guild_id.unwrap();

    let guild = match ctx.cache.guild(guild_id, |guild| guild.into()) {
        Ok(guild) => guild,
        Err(err) => {
            let _ = command.error(&ctx, GENERAL_ISSUE).await;

            return Err(err.into());
        }
    };

    let args = match ServerConfigCommandKind::slash(&command)? {
        ServerConfigCommandKind::Args(args) => args,
        ServerConfigCommandKind::Auth(args) => {
            return super::_authorities(ctx, command.into(), args).await
        }
    };

    if args.any() {
        let f = |config: &mut GuildConfig| {
            let ServerConfigArgs {
                embeds_maximized,
                profile_size,
                show_retries,
                togglesongs,
            } = args;

            if let Some(embeds) = embeds_maximized {
                config.embeds_maximized = Some(embeds);
            }

            if let Some(profile) = profile_size {
                config.profile_size = Some(profile);
            }

            if let Some(retries) = show_retries {
                config.show_retries = Some(retries);
            }

            if let Some(with_lyrics) = togglesongs {
                config.with_lyrics = Some(with_lyrics);
            }
        };

        if let Err(err) = ctx.update_guild_config(guild_id, f).await {
            let _ = command.error(&ctx, GENERAL_ISSUE).await;

            return Err(err);
        }
    }

    let config = ctx.guild_config(guild_id).await;
    let mut authorities = Vec::with_capacity(config.authorities.len());

    for &auth in &config.authorities {
        if let Some(Ok(name)) =
            Id::new_checked(auth).map(|role| ctx.cache.role(role, |role| role.name.to_owned()))
        {
            authorities.push(name);
        }
    }

    let embed = ServerConfigEmbed::new(guild, config, &authorities);
    let builder = embed.into_builder().build().into();
    command.create_message(&ctx, builder).await?;

    Ok(())
}

pub fn define_serverconfig() -> MyCommand {
    let role =
        MyCommandOption::builder("role", "Specify the role that should gain authority status")
            .role(true);

    let add = MyCommandOption::builder("add", "Add authority status to a role")
        .help("Add authority status to a role.\nServers can have at most 10 authority roles.")
        .subcommand(vec![role]);

    let list = MyCommandOption::builder("list", "Display all current authority roles")
        .subcommand(Vec::new());

    let role =
        MyCommandOption::builder("role", "Specify the role that should lose authority status")
            .role(true);

    let remove_help = "Remove authority status from a role.\n\
        You can only use this if the removed role would __not__ make you lose authority status yourself.";

    let remove = MyCommandOption::builder("remove", "Remove authority status from a role")
        .help(remove_help)
        .subcommand(vec![role]);

    let authorities_help = "To use certain commands, users require a special status.\n\
        This command adjusts the authority status of roles.\n\
        Any member with an authority role can use these higher commands.\n\n\
        Authority commands: `matchlive`, `prune`, `roleassign`, \
        `serverconfig`, `track`, `trackstream`.";

    let authorities =
        MyCommandOption::builder("authorities", "Adjust authority roles for a server")
            .subcommandgroup(vec![add, list, remove])
            .help(authorities_help);

    let song_commands_description = "Choose whether song commands can be used or not";

    let song_commands_choices = vec![
        CommandOptionChoice::String {
            name: "enable".to_owned(),
            value: "enable".to_owned(),
        },
        CommandOptionChoice::String {
            name: "disable".to_owned(),
            value: "disable".to_owned(),
        },
    ];

    let song_commands = MyCommandOption::builder("song_commands", song_commands_description)
        .string(song_commands_choices, false);

    let profile_description = "What initial size should the profile command be?";

    let profile_choices = vec![
        CommandOptionChoice::String {
            name: "compact".to_owned(),
            value: "compact".to_owned(),
        },
        CommandOptionChoice::String {
            name: "medium".to_owned(),
            value: "medium".to_owned(),
        },
        CommandOptionChoice::String {
            name: "full".to_owned(),
            value: "full".to_owned(),
        },
    ];

    let profile_help = "What initial size should the profile command be?\n\
        Applies only if the member has not specified a config for themselves.";

    let profile = MyCommandOption::builder(PROFILE, profile_description)
        .string(profile_choices, false)
        .help(profile_help);

    let embeds_description =
        "What initial size should the recent, compare, simulate, ... commands be?";

    let embeds_help = "Some embeds are pretty chunky and show too much data.\n\
        With this option you can make those embeds minimized by default.\n\
        Affected commands are: `compare score`, `recent score`, `recent simulate`, \
        and any command showing top scores when the `index` option is specified.\n\
        Applies only if the member has not specified a config for themselves.";

    let embeds_choices = vec![
        CommandOptionChoice::String {
            name: "maximized".to_owned(),
            value: "maximized".to_owned(),
        },
        CommandOptionChoice::String {
            name: "minimized".to_owned(),
            value: "minimized".to_owned(),
        },
    ];

    let embeds = MyCommandOption::builder("embeds", embeds_description)
        .help(embeds_help)
        .string(embeds_choices, false);

    let retries_description = "Should the amount of retries be shown for the `recent` command?";
    let retries_help = "Should the amount of retries be shown for the `recent` command?\n\
            Applies only if the member has not specified a config for themselves.";

    let retries_choices = vec![
        CommandOptionChoice::String {
            name: "show".to_owned(),
            value: "show".to_owned(),
        },
        CommandOptionChoice::String {
            name: "hide".to_owned(),
            value: "hide".to_owned(),
        },
    ];

    let retries = MyCommandOption::builder("retries", retries_description)
        .string(retries_choices, false)
        .help(retries_help);

    let edit = MyCommandOption::builder("edit", "Adjust configurations for a server")
        .subcommand(vec![song_commands, profile, embeds, retries]);

    let description = "Adjust configurations or authority roles for this server";

    MyCommand::new("serverconfig", description).options(vec![authorities, edit])
}