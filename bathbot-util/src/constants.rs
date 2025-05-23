use twilight_model::id::{Id, marker::UserMarker};

// Colors
pub const DARK_GREEN: u32 = 0x1F8B4C;
pub const RED: u32 = 0xE74C3C;

// Message field sizes
pub const DESCRIPTION_SIZE: usize = 4096;
pub const FIELD_VALUE_SIZE: usize = 1024;

// osu!
pub const OSU_BASE: &str = "https://osu.ppy.sh/";
/// FIXME: Endpoint is sometimes wrong so avoid using it, see issue #426
pub const MAP_THUMB_URL: &str = "https://b.ppy.sh/thumb/";
pub const AVATAR_URL: &str = "https://a.ppy.sh/";
pub const HUISMETBENEN: &str = "https://api.snipe.huismetbenen.nl/";
pub const KITTENROLEPLAY: &str = "https://flashlight.moe/api/snipes/";
pub const RELAX_API: &str = "https://rx.stanr.info/api";
pub const RELAX: &str = "https://rx.stanr.info";
pub const RELAX_ICON_URL: &str = "https://rx.stanr.info/rv-yellowlight-192.png";

// twitch
pub const TWITCH_BASE: &str = "https://www.twitch.tv/";
pub const TWITCH_STREAM_ENDPOINT: &str = "https://api.twitch.tv/helix/streams";
pub const TWITCH_USERS_ENDPOINT: &str = "https://api.twitch.tv/helix/users";
pub const TWITCH_VIDEOS_ENDPOINT: &str = "https://api.twitch.tv/helix/videos";
pub const TWITCH_OAUTH: &str = "https://id.twitch.tv/oauth2/token";

// Error messages
pub const GENERAL_ISSUE: &str = "Something went wrong, blame bade";
pub const OSU_API_ISSUE: &str = "Some issue with the osu api, blame bade";
pub const ORDR_ISSUE: &str = "Some issue with the o!rdr api, blame bade";
pub const OSEKAI_ISSUE: &str = "Some issue with the osekai api, blame bade";
pub const OSUSTATS_API_ISSUE: &str = "Some issue with the osustats api, blame bade";
pub const TWITCH_API_ISSUE: &str = "Some issue with the twitch api, blame bade";
pub const THREADS_UNAVAILABLE: &str = "Cannot start new thread from here";

// Discord error codes
pub const CANNOT_DM_USER: u64 = 50007;
pub const INVALID_ACTION_FOR_CHANNEL_TYPE: u64 = 50024;
pub const MESSAGE_TOO_OLD_TO_BULK_DELETE: u64 = 50034;

pub const UNKNOWN_CHANNEL: u64 = 10003;

// Misc
pub const INVITE_LINK: &str = "https://discord.com/api/oauth2/authorize?client_id=297073686916366336&permissions=309238025216&scope=bot%20applications.commands";
pub const BATHBOT_WORKSHOP: &str = "https://discord.gg/n9fFstG";
pub const BATHBOT_GITHUB: &str = "https://github.com/MaxOhn/Bathbot";
pub const BATHBOT_ROADMAP: &str = "https://github.com/users/MaxOhn/projects/3";
pub const KOFI: &str = "https://ko-fi.com/bathbot";
pub const MISS_ANALYZER_ID: Id<UserMarker> = Id::new(752035690237394944);
