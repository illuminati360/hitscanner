mod iputils;

use iputils::IPLabeller;
use trie::common::{NoMeta, Prefix};

use std::{
    io::{stdin, BufRead},
    path::PathBuf,
};
use std::net::Ipv4Addr;

use crate::iputils::IPRange;

const HELP: &str = "\
Usage: iplabel

OPTIONS:
    -g         merged.db / merged.csv file
INPUT:
    stdin each line is an IPv4Addr
OUTPUT:
    labelled IPv4Addr. e.g.
    114.114.114.114 0,CN,1,CN,2,CN,3,CN,4,CN,5,CN
";

#[allow(dead_code)]
struct AppArgs {
    geo: PathBuf,
}

fn parse_path(s: &std::ffi::OsStr) -> Result<PathBuf, &'static str> {
    Ok(s.into())
}

fn getoption() -> Result<AppArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = AppArgs {
        geo: pargs.value_from_os_str(["-g", "--geo"], parse_path)?,
    };

    Ok(args)
}

type Prefix4NoMeta<'a> = Prefix<u32, NoMeta>;

fn main() {
    let args = match getoption() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            std::process::exit(1);
        }
    };

    let mut pfxs: Vec<Prefix<u32, String>> = vec![];
    let geo_labeller: IPLabeller<IPRange> = IPLabeller::new(&args.geo, &mut pfxs);

    for l in stdin().lock().lines() {
        // destination IP address
        let ip: Ipv4Addr = l.as_ref().unwrap().parse().unwrap();
        let p = Prefix4NoMeta::new(ip.into(), 32);
        let r = geo_labeller.match_pfx(&p);
        let db_geos = r.unwrap().meta.as_ref().unwrap();
        println!("{} {}", ip, db_geos);
    }
}
