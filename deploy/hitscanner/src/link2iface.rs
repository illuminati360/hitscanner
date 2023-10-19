// trace2link -- extract Router IPs from a links file
// =============================================================================
// USAGE: trace2link <$path_to_links_file>
// INPUT: a links file from STDIN or @ARGV
// OUTPUT: a list Router IPs

use itertools::Itertools;
use std::io::{BufRead, BufReader, Read};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn openfile(path: &std::path::PathBuf) -> BufReader<Box<dyn Read>> {
    let input: Box<dyn std::io::Read + 'static> = if path.as_os_str() == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(&path).unwrap())
    };

    let reader: BufReader<Box<dyn Read>> = BufReader::new(input);

    return reader;
}

fn main(){
    let mut pargs = pico_args::Arguments::from_env();
    let input = match pargs.free_from_str::<PathBuf>() {
        Ok(v) => v,
        Err(_) => PathBuf::from("-"),
    };

    let file = openfile(&input);
    let mut out: HashMap<String, bool> = HashMap::new();
    let mut router = HashSet::new();
    for line in file.lines() {
        let l: Vec<String> = line.unwrap().split_whitespace().map(|x| x.to_string()).collect();
        let is_dest = if l[2] == "Y" { true } else { false };
        router.insert(l[0].to_string());
        if !out.contains_key(&l[1]) {
            out.insert(l[1].to_string(), is_dest);
        } else if !is_dest {
            out.insert(l[1].to_string(), false);
        }
    }
    for i in out.keys() {
        if !out[i] {
            router.insert(i.to_string());
        }
    }
    for i in router.iter().sorted() {
        println!("{}", i);
    }
}