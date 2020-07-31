use super::{try_link_name, Args};
use crate::{
    util::{matcher, osu::ModSelection},
    Context,
};

use itertools::Itertools;
use std::str::FromStr;

pub struct NameArgs {
    pub name: Option<String>,
}

impl NameArgs {
    pub fn new(ctx: &Context, mut args: Args) -> Self {
        let name = try_link_name(ctx, args.next());
        Self { name }
    }
}

pub struct MultNameArgs {
    pub names: Vec<String>,
}

impl MultNameArgs {
    pub fn new(ctx: &Context, args: Args, n: usize) -> Self {
        let names = args
            .take(n)
            .unique()
            .map(|arg| try_link_name(ctx, Some(arg)).unwrap())
            .collect();
        Self { names }
    }
}

pub struct NameFloatArgs {
    pub name: Option<String>,
    pub float: f32,
}

impl NameFloatArgs {
    pub fn new(ctx: &Context, args: Args) -> Result<Self, &'static str> {
        let mut args = args.take_all();
        let float = match args.next_back().and_then(|arg| f32::from_str(&arg).ok()) {
            Some(float) => float,
            None => return Err("You need to provide a decimal number as last argument"),
        };
        let name = try_link_name(ctx, args.next());
        Ok(Self { name, float })
    }
}

pub struct NameIntArgs {
    pub name: Option<String>,
    pub number: Option<u32>,
}

impl NameIntArgs {
    pub fn new(ctx: &Context, args: Args) -> Self {
        let mut name = None;
        let mut number = None;
        for arg in args {
            let res = u32::from_str(arg).ok();
            if res.is_some() {
                number = res;
            } else {
                name = try_link_name(ctx, Some(arg));
            }
        }
        Self { name, number }
    }
}

pub struct NameModArgs {
    pub name: Option<String>,
    pub mods: Option<ModSelection>,
}

impl NameModArgs {
    pub fn new(ctx: &Context, args: Args) -> Self {
        let mut name = None;
        let mut mods = None;
        for arg in args {
            let res = matcher::get_mods(arg);
            if res.is_some() {
                mods = res;
            } else {
                name = try_link_name(ctx, Some(arg));
            }
        }
        Self { name, mods }
    }
}
