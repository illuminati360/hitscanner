// dbmerge -- merge a number of .db geoIPDB files into a single one
// =============================================================================
// USAGE: dbmerge [dot_db_file]
// INPUT: a batch of .db geoIPDB files, with the following format:
//            "3758095872","3758096127","SG","Singapore","Singapore","Singapore","Marina Bay Sands Pte Ltd","marinabaysands.com"
//        a valid .db file should satisfy:
//             1. Intervals don't overlap
//             2. Lines are sorted by Intervals
// OUTPUT: .db file with only country code annotation, e.g.:
//         3758096128,3758096383,0,"AU",1,"AU",2,"AU"
// NOTE:   the algorithm uses [a,b), i.e. left-closed and right-open interval
//         while .db files use [a,b-1], i.e. closed interval

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::result::Result;

const HELP: &str = "\
Usage: dbmerge [OPTIONS] <files>

OPTIONS:
-s   split the ranges even if the country codes are the same
";

#[allow(dead_code)]
struct AppArgs {
    split: bool,
    inputs: Vec<std::ffi::OsString>,
}

fn get_option() -> Result<AppArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = AppArgs {
        split: pargs.contains(["-s", "--split"]),
        inputs: pargs.finish(),
    };

    if args.inputs.is_empty() {
        print!("{}", HELP);
        std::process::exit(0);
    }

    Ok(args)
}

#[derive(Debug)]
struct Range {
    a: u64,
    b: u64,
    g: String,
}

fn parse_line(line: &str) -> Option<Range> {
    if line.is_empty() {
        return None;
    }
    let fields: Vec<&str> = line.trim().split(',').collect();
    let mut g = fields[2].replace("\"", "").to_string();
    if g.is_empty() {
        g = "-".to_string();
    }

    let a = fields[0].replace("\"", "").parse::<u64>().unwrap();
    let b = fields[1].replace("\"", "").parse::<u64>().unwrap();
    Some(Range {
        a,
        b: b + 1, // the algorithm uses [a,b), i.e. left-closed and right-open interval
        g,
    })
}

fn min_front(tl: &Vec<Option<Range>>, il: &Vec<usize>) -> (u64, isize, usize) {
    let mut n = u64::MAX;
    let mut i = -1;
    let mut j = 0;

    for (ii, l) in tl.iter().enumerate() {
        let jj = il[ii];
        if let Some(range) = l {
            let front = if jj % 2 == 0 { range.a } else { range.b };
            if front < n || (front == n && jj % 2 == 0) {
                // jj%2 == 0 means the tick is the right end of the interval, which is open and thus smaller than closed n
                n = front;
                i = ii as isize;
                j = jj;
            }
        }
    }
    (n, i, j)
}

// The merge_ticks function leverages the sorted input .db files to merge them efficiently.
// It traverses the IP space from 0, moving from tick to tick.
// To find the next tick, it scans the top line of each .db file and selects the smallest value.
// The interval annotation is determined by combining current annotations from each .db file.
// To track the current annotation for each .db file, the tick index's parity is used, corresponding to interval "entry" and "exit" events.
fn merge_ticks(ll: &mut Vec<BufReader<File>>, split: bool) {
    // tick index list
    let mut il = vec![0; ll.len()];
    // temporary line list
    let mut tl: Vec<Option<Range>> = ll
        .iter_mut()
        .map(|reader| {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            parse_line(&line)
        })
        .collect();

    // annotation list
    let mut al: Vec<Option<(usize, usize, String)>> = vec![None; ll.len()];

    // previous tick number
    let mut pn = None;

    // previous annotation
    let mut pg: Option<Vec<String>> = None;
    let mut a: Option<u64> = None;

    loop {
        let (n, i, j) = min_front(&tl, &il);
        if a.is_none() {
            a = Some(n);
        }
        if i == -1 {
            if !split && pg.is_some() {
                println!(
                    "{},{},{}",
                    a.unwrap(),
                    pn.unwrap() - 1,
                    pg.unwrap().join(",")
                );
            }
            break;
        }

        if let Some(pn_value) = pn {
            if n - pn_value > 0 {
                let fl: Vec<(usize, usize, String)> =
                    al.iter().filter_map(|x| x.as_ref().cloned()).collect();

                if !fl.is_empty() {
                    let g: Vec<String> = fl.iter().map(|f| format!("{},{}", f.0, f.2)).collect();
                    if !split && pg.is_some() && pg.as_ref().unwrap() != &g {
                        println!(
                            "{},{},{}",
                            a.unwrap(),
                            pn.unwrap() - 1,
                            pg.as_ref().unwrap().join(",")
                        ); // n-1 because .db file uses closed interval, i.e. [a,b]
                        a = Some(pn_value);
                    } else if split {
                        println!("{},{},{}", pn_value, n - 1, g.join(",")); // n-1 because .db file uses closed interval, i.e. [a,b]
                    }
                    pg = Some(g);
                }
            }
        }

        al[i as usize] = if j % 2 == 0 {
            Some((i as usize, j, tl[i as usize].as_ref().unwrap().g.clone()))
        } else {
            None
        };
        pn = Some(n);

        il[i as usize] += 1;
        if il[i as usize] % 2 == 0 {
            let mut line = String::new();
            if ll[i as usize].read_line(&mut line).unwrap() > 0 {
                tl[i as usize] = parse_line(&line);
            } else {
                tl[i as usize] = None;
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = get_option().unwrap();

    let mut ll: Vec<BufReader<File>> = Vec::new();
    for db_file in &args.inputs {
        let file = File::open(db_file)?;
        let reader = BufReader::new(file);
        ll.push(reader);
    }

    merge_ticks(&mut ll, args.split);

    Ok(())
}
