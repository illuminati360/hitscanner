mod iputils;

use iputils::parse_prefix_str;
use trie::common::{NoMeta, Prefix};

use std::{
    cmp::{max, min},
    fs,
    io::{stdin, BufRead},
    path::PathBuf,
};

use rand::{thread_rng, Rng};
use serde_json::Value;

const HELP: &str = "\
Usage: ipsample [OPTIONS]

OPTIONS:
    -c         path to task config
    -t         type: UNIFORM, RANDOM_UNIFORM
    -d         density
    -o         offset
";

struct AppArgs {
    config: Option<PathBuf>,
    r#type: Option<String>,
    density: Option<u8>,
    offset: Option<u32>,
}

fn parse_path(s: &std::ffi::OsStr) -> Result<PathBuf, &'static str> {
    Ok(s.into())
}

fn get_option() -> Result<AppArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = AppArgs {
        config: pargs.opt_value_from_os_str(["-c", "--config"], parse_path)?,
        density: pargs.opt_value_from_str(["-d", "--density"])?,
        r#type: pargs.opt_value_from_str(["-t", "--type"])?,
        offset: pargs.opt_value_from_str(["-o", "--offset"])?,
    };

    if args.config.is_none()
        && args.density.is_none()
        && args.r#type.is_none()
        && args.offset.is_none()
    {
        print!("{}", HELP);
        std::process::exit(0);
    }

    Ok(args)
}

fn random_uniform_sample(p: Prefix<u32, NoMeta>, density: u8) {
    let a = p.net; // start address
    let g: u32 = 1u32 << (32 - max(density, p.len)); // granularity, (density and p.len both > 0)
    let n = 1u32 << 32 - &p.len;
    let mut rng = thread_rng();
    let mut o = rng.gen_range(0..g); // offset is randomly chosen
    o = max(0, min(o, n - 1)); // clamp offset between [0, n-1] to make sure it's inside the prefix range

    for i in (0..n).step_by(g as usize) {
        println!("{}", std::net::Ipv4Addr::from(a + (i + o) % n));
    }
}

fn uniform_sample(p: Prefix<u32, NoMeta>, density: u8, offset: u32) {
    let a = p.net; // start address
    let g: u32 = 1u32 << (32 - max(density, p.len)); // granularity, (density and p.len both > 0)
    let n = 1u32 << 32 - &p.len;
    let o = max(0, min(offset, n - 1)); // clamp offset between [0, n-1] to make sure it's inside the prefix range

    for i in (0..n).step_by(g as usize) {
        println!("{}", std::net::Ipv4Addr::from(a + (i + o) % n));
    }
}

fn main() {
    let args = match get_option() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            std::process::exit(1);
        }
    };

    let (r#type, density, offset): (String, u8, u32);
    if args.config.is_some() {
        let text = fs::read_to_string(args.config.unwrap()).unwrap();

        let json: Value = serde_json::from_str(&text).unwrap();
        r#type = json["target"]["sample"]["type"]
            .as_str()
            .expect("No sampling type given")
            .to_string();
        density = json["target"]["sample"]["density"]
            .as_u64()
            .expect("No sampling density given")
            .try_into()
            .unwrap();
        offset = json["target"]["sample"]["type"]
            .as_str()
            .expect("No sampling offset given")
            .parse()
            .unwrap();
    } else {
        r#type = args.r#type.expect("No sampling type given");
        density = args.density.expect("No sampling density given");
        offset = args.offset.expect("No sampling offset given");
    }

    for l in stdin().lock().lines() {
        // destination IP address
        let p = parse_prefix_str(l.as_ref().unwrap());
        match r#type.as_str() {
            "UNIFORM" => uniform_sample(p, density, offset),
            "RANDOM_UNIFORM" => random_uniform_sample(p, density),
            _ => {}
        };
    }
}
