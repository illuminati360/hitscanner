mod iputils;

use iputils::IPLabeller;
use trie::common::{NoMeta, Prefix};

use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::iputils::IPRange;

const HELP: &str = "\
Usage: trace2mat [OPTIONS] [files]

When [files] is empty, read file names from STDIN
OPTIONS:
    -b         path to routeviews.csv
    -g         path to merged.db / merged.csv
    -i         path to .iface
    -a         country code (ISO 3166-1 alpha-2 standard)
OUTPUTS: output as a sparse matrix
    row.csv    each row represent a trace destination: number,IP,signature,key
    col.csv    each col represent a router: number,IP
    mat.csv    each line represent a 1 in matrix: row, col
EXAMPLE:
    cat sorted_traceroute_lines.txt | trace2mat -b routeviews.csv -g merged.db -i ifaces -a HK
";

fn parse_sig(db_geos: &String, db_num: usize, area: &str) -> String {
    let mut sig = vec!['0'; db_num];
    let f: Vec<&str> = db_geos.split(",").collect();
    for i in (0..f.len()).step_by(2) {
        sig[f[i].parse::<usize>().unwrap()] = if f[i + 1] == area { '1' } else { '0' }
    }
    sig.iter().collect()
}

#[allow(dead_code)]
struct AppArgs {
    geo: PathBuf,
    iface: PathBuf,
    area: PathBuf,
    inputs: Vec<std::ffi::OsString>,
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
        iface: pargs.value_from_os_str(["-i", "--iface"], parse_path)?,
        area: pargs.value_from_os_str(["-a", "--area"], parse_path)?,
        inputs: pargs.finish(),
    };

    Ok(args)
}

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
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

#[derive(Clone, Debug)]
struct Link {
    io: InOut,
}

impl Link {
    fn new() -> Self {
        Self { io: InOut::new() }
    }
}

fn add_link(
    area: &str,
    link: &[Link],
    dst: Ipv4Addr,
    row: &mut Vec<u64>,
    col: &mut Vec<u64>,
    dst2row: &mut HashMap<Ipv4Addr, u64>,
    rtr2col: &mut HashMap<Ipv4Addr, u64>,
    nodes: &mut HashMap<Ipv4Addr, (String, String)>,
    ifaces: &HashSet<Ipv4Addr>,
    geo_labeller: &IPLabeller<IPRange>,
) {
    if !dst2row.contains_key(&dst) {
        dst2row.insert(dst, dst2row.len() as u64);
    }

    let mut t: HashSet<Ipv4Addr> = HashSet::new();
    for l in link {
        for ip in [
            l.io._in.parse::<Ipv4Addr>().unwrap(),
            l.io.out.parse::<Ipv4Addr>().unwrap(),
        ] {
            let p = Prefix4NoMeta::new(ip.into(), 32);
            let r = geo_labeller.match_pfx(&p);
            let db_geos = r.unwrap().meta.as_ref().unwrap();
            let sig = parse_sig(db_geos, 6, area);
            let key = format!(
                "{}/{}",
                std::net::Ipv4Addr::from(r.unwrap().net),
                r.unwrap().len
            );

            if !nodes.contains_key(&ip) {
                nodes.insert(ip, (sig.clone(), key));
                if sig.contains('1') && ifaces.contains(&ip) {
                    rtr2col.insert(ip, rtr2col.len() as u64);
                }
            }
            if sig.contains('1') && ifaces.contains(&ip) && !t.contains(&ip) {
                t.insert(ip);
                row.push(*dst2row.get(&dst).unwrap());
                col.push(*rtr2col.get(&ip).unwrap());
            }
        }
    }
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

fn open_file(path: &std::path::PathBuf) -> Box<dyn std::io::Read + 'static> {
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

fn process(
    read: &mut Box<dyn std::io::Read>,
    area: &str,
    ifaces: &HashSet<Ipv4Addr>,
    geo_labeller: &IPLabeller<IPRange>,
    row: &mut Vec<u64>,
    col: &mut Vec<u64>,
    dst2row: &mut HashMap<Ipv4Addr, u64>,
    rtr2col: &mut HashMap<Ipv4Addr, u64>,
    nodes: &mut HashMap<Ipv4Addr, (String, String)>,
) {
    let mut l = Link::new();
    let mut link = Vec::new();
    let mut node = HashMap::new();

    let mut is_loop = false;
    let mut prev_dest: Option<Ipv4Addr> = None;
    let mut last: Vec<String> = Vec::new();

    for line in BufReader::new(read).lines() {
        let ll = line.unwrap();
        let line = ll.trim();
        let f: Vec<String> = line.split_whitespace().map(|x| x.to_string()).collect();
        if &f[1] == "*" {
            continue;
        }
        if f[0].chars().next().unwrap() == 't' {
            if prev_dest.is_some() {
                add_link(
                    area,
                    &link,
                    prev_dest.unwrap().clone(),
                    row,
                    col,
                    dst2row,
                    rtr2col,
                    nodes,
                    &ifaces,
                    geo_labeller,
                );
            }
            prev_dest = Some(f[4].parse().unwrap());
            node.clear();
            link.clear();
            is_loop = false;
        } else if last.len() > 1 && last[1] != "from" && f[1] != last[1] {
            if node.contains_key(&f[1]) {
                is_loop = true;
            }
            node.insert(f[1].to_string(), true);
            l.io._in = last[1].to_string();
            l.io.out = f[1].to_string();
            if !is_loop {
                link.push(l.clone());
            }
        }
        last = f.clone();
    }

    add_link(
        area,
        &link,
        prev_dest.unwrap().clone(),
        row,
        col,
        dst2row,
        rtr2col,
        nodes,
        &ifaces,
        geo_labeller,
    );
}

type Prefix4NoMeta<'a> = Prefix<u32, NoMeta>;

fn main() {
    let mut args = match getoption() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            std::process::exit(1);
        }
    };

    let inputs = if args.inputs.is_empty() {
        vec![std::ffi::OsString::from("-")]
    } else {
        let mut seen = std::collections::HashSet::new();
        args.inputs.retain(|x| seen.insert(x.clone()));
        args.inputs
    };

    let mut pfxs: Vec<Prefix<u32, String>> = vec![];
    let geo_labeller: IPLabeller<IPRange> = IPLabeller::new(&args.geo, &mut pfxs);

    let mut ifaces: HashSet<Ipv4Addr> = HashSet::new();
    for l in BufReader::new(open_file(&PathBuf::from(&args.iface))).lines() {
        ifaces.insert(l.unwrap().parse().unwrap());
    }

    let mut row: Vec<u64> = Vec::new();
    let mut col: Vec<u64> = Vec::new();
    let mut dst2row: HashMap<Ipv4Addr, u64> = HashMap::new();
    let mut rtr2col: HashMap<Ipv4Addr, u64> = HashMap::new();
    let mut nodes: HashMap<Ipv4Addr, (String, String)> = HashMap::new();

    for input in inputs {
        let mut file = open_file(&PathBuf::from(&input));
        process(
            &mut file,
            args.area.to_str().unwrap(),
            &ifaces,
            &geo_labeller,
            &mut row,
            &mut col,
            &mut dst2row,
            &mut rtr2col,
            &mut nodes,
        );
    }

    let mut f = File::create("rows.csv").unwrap();
    let mut row2ind: HashMap<u64, u64> = HashMap::new(); // keep track of original index used in row
    for (i, k) in dst2row.keys().sorted().enumerate() {
        let p = Prefix4NoMeta::new((*k).into(), 32);
        let r = geo_labeller.match_pfx(&p);
        let db_geos = r.unwrap().meta.as_ref().unwrap();
        let sig = parse_sig(db_geos, 6, args.area.to_str().unwrap());
        let key = format!(
            "{}/{}",
            std::net::Ipv4Addr::from(r.unwrap().net),
            r.unwrap().len
        );
        let line = format!("{},{},{},{}\n", i, k, sig, key);
        row2ind.insert(*dst2row.get(k).unwrap(), i as u64);
        f.write_all(line.as_bytes()).unwrap();
    }

    f = File::create("mat.csv").unwrap();
    let mut col2ind: HashMap<u64, u64> = HashMap::new(); // keep track of appearance order of a router index (i.e. col)
    let ind_sorted = (0..row.len()).sorted_by_key(|x| row2ind.get(&row[*x]).unwrap());
    for i in ind_sorted {
        let ind = row2ind.get(&row[i]).unwrap();
        if !col2ind.contains_key(&col[i]) {
            col2ind.insert(col[i], col2ind.len() as u64);
        }
        let line = format!("{},{}\n", ind, col2ind.get(&col[i]).unwrap());
        f.write_all(line.as_bytes()).unwrap();
    }

    f = File::create("cols.csv").unwrap();
    for k in rtr2col.keys().sorted_by_key(|x| col2ind.get(rtr2col.get(x).unwrap()).unwrap()) {
        let line = format!("{},{}\n", col2ind.get(rtr2col.get(k).unwrap()).unwrap(), k);
        f.write_all(line.as_bytes()).unwrap();
    }
}
