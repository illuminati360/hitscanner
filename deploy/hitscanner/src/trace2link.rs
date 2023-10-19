// trace2link -- extract IP-level links from a batch of traceroute data files
// =============================================================================
// USAGE: see Usage below (./trace2link -h)
// INPUT: a batch of traceroute data file names from STDIN or @ARGV
//        the file format is .warts.gz or warts2text
// INPUT: a batch of traceroute data file names from STDIN or @ARGV
//        the file format is .warts.gz or warts2text
// OUTPUT: CSV text
//         1.in 2.out 3.is_dest 4.star 5.delay 6.freq 7.ttl 8.monitor
//         1. the IP address of the ingress interface, e.g., 1.2.3.4
//         2. the IP address of the outgress interface, e.g., 5.6.7.8
//         3. whether the outgress node is the destination, e.g., Y or N
//         4. the number of anonymous (*) hops inbetween, e.g., 0 for directed link
//         5. the minimal delay in ms > 0, e.g., 10
//         6. the cumulative frequence of link observed, e.g., 5000
//         7. the minimal TTL of the ingress interface, e.g., 7
//         8. the monoitor which observed the link at the minimal TTL, e.g., 9.0.1.2

use itertools::Itertools;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

// command line arguments
const HELP: &str = "\
Usage: trace2link.pl [OPTIONS] [files]

When [files] is empty, read file names from STDIN
OPTIONS:
-    read txt-format warts2text data from STDIN
-h   print this help message
-p   the prefix of output file names
-z   output with gzip
";

#[allow(dead_code)]
struct AppArgs {
    prefix: Option<std::path::PathBuf>,
    gzip: bool,
    inputs: Vec<std::ffi::OsString>,
}

fn parse_path(s: &std::ffi::OsStr) -> Result<std::path::PathBuf, &'static str> {
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
        prefix: pargs.opt_value_from_os_str(["-p", "--prefix"], parse_path)?,
        gzip: pargs.contains(["-z", "--gzip"]),
        inputs: pargs.finish(),
    };

    Ok(args)
}

// I/O helpers
fn filetype(path_string: &str) -> &str {
    if path_string.ends_with("warts")
        || path_string.ends_with("warts.gz")
        || path_string.ends_with("warts.g")
    {
        return "BIN";
    }
    return "TXT";
}

fn openfile(path: &std::path::PathBuf) -> Box<dyn std::io::Read + 'static> {
    let input: Box<dyn std::io::Read + 'static> = if path.as_os_str() == "-" {
        Box::new(std::io::stdin())
    } else {
        let path_string = path.as_os_str().to_str().unwrap().to_string();
        let mut cmd;
        if filetype(&path_string) == "BIN" {
            cmd = format!("{} | sc_warts2text", path_string);
        } else {
            cmd = format!("{}", path_string);
        }

        if path_string.ends_with(".gz") || path_string.ends_with(".g") {
            cmd = format!("gzip -dc {}", cmd);
        } else {
            cmd = format!("cat {}", cmd);
        }
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let o = child.stdout.take().unwrap();
        Box::new(o)
    };

    return input;
}

// sub-routines
#[derive(Clone)]
struct Link {
    io: InOut,
    prop: LinkProp,
}

impl Link {
    fn new() -> Self {
        Self {
            io: InOut::new(),
            prop: LinkProp::new(),
        }
    }
}

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct InOut {
    _in: String,
    out: String,
}

impl InOut {
    fn new() -> Self {
        Self {
            _in: String::new(),
            out: String::new(),
        }
    }
}

#[derive(Clone)]
struct LinkProp {
    is_dest: bool,
    star: u32,
    delay: f64,
    freq: u32,
    ttl: u32,
    monitor: String,
    firstseen: u32,
    lastseen: u32,
}

impl LinkProp {
    fn new() -> Self {
        Self {
            is_dest: false,
            star: 0,
            delay: 0.0,
            freq: 0,
            ttl: 0,
            monitor: String::new(),
            firstseen: 0,
            lastseen: 0,
        }
    }
}

fn addlink(link: &Vec<Link>, links: &mut HashMap<InOut, LinkProp>) {
    for l in link {
        if links.contains_key(&l.io) {
            let a = links.get_mut(&l.io).unwrap();
            if l.prop.is_dest == false {
                a.is_dest = false
            };
            if a.star > l.prop.star {
                a.star = l.prop.star
            };
            if a.delay > l.prop.delay {
                a.delay = l.prop.delay
            };
            a.freq += 1;
            if a.ttl > l.prop.ttl || (a.ttl == l.prop.ttl && l.prop.monitor < a.monitor) {
                a.monitor = l.prop.monitor.clone();
                a.ttl = l.prop.ttl;
            }
            if a.firstseen > l.prop.firstseen {
                a.firstseen = l.prop.firstseen
            };
            if a.lastseen < l.prop.lastseen {
                a.lastseen = l.prop.lastseen
            };
        } else {
            links.insert(l.io.clone(), l.prop.clone());
        }
    }
}

fn process(read: &mut Box<dyn std::io::Read>, links: &mut HashMap<InOut, LinkProp>) {
    let (mut dest, mut start, mut last, mut node, mut link, mut is_loop): (
        String,
        u32,
        Vec<String>,
        HashMap<String, bool>,
        Vec<Link>,
        bool,
    ) = (
        String::from(""),
        0,
        Vec::new(),
        HashMap::new(),
        Vec::new(),
        false,
    );

    let mut l = Link::new();

    for line in BufReader::new(read).lines() {
        let ll = line.unwrap();
        let line = ll.trim();
        let f: Vec<String> = line.split_whitespace().map(|x| x.to_string()).collect();
        if &f[1] == "*" {
            l.prop.star += 1;
            continue;
        }
        if f[0].chars().next().unwrap() == 't' {
            if !is_loop {
                addlink(&link, links);
            }
            l.prop.monitor = f[2].to_string();
            dest = f[4].clone();
            start = f[5].parse::<u32>().unwrap();
            node = HashMap::new();
            link = Vec::new();
            is_loop = false;
        } else if last.len() > 1 && last[1] != "from" && f[1] != last[1] {
            if node.contains_key(&f[1]) {
                is_loop = true;
            }
            node.insert(f[1].to_string(), true);
            l.io._in = last[1].to_string();
            l.io.out = f[1].to_string();
            l.prop.is_dest = if l.io.out == dest { true } else { false };
            l.prop.delay = (f[2].parse::<f64>().unwrap() - last[2].parse::<f64>().unwrap()) / 2.0;
            l.prop.delay = l.prop.delay.max(0.0);
            l.prop.freq = 1;
            l.prop.ttl = last[0].parse::<u32>().unwrap();
            l.prop.firstseen = start;
            l.prop.lastseen = start;
            link.push(l.clone());
        }
        l.prop.star = 0;
        last = f.clone();
    }
    if !is_loop {
        addlink(&link, links);
    }
}

fn main() {
    let mut args = match getoption() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            std::process::exit(1);
        }
    };

    if args.inputs.len() <= 0 {
        args.inputs = vec![std::ffi::OsString::from("-")];
    }

    let mut links: HashMap<InOut, LinkProp> = HashMap::new();
    for input in args.inputs {
        let mut file = openfile(&PathBuf::from(&input));
        process(file.by_ref(), &mut links);
    }

    for key in links.keys().sorted() {
        let value = &links[key];
        println!(
            "{} {} {} {} {:.3} {} {} {} {} {}",
            key._in,
            key.out,
            if value.is_dest { "Y" } else { "N" },
            value.star,
            value.delay,
            value.freq,
            value.ttl,
            value.monitor,
            value.firstseen,
            value.lastseen
        );
    }
}
