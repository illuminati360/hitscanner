// linkmerge -- merge a batch of link files
// =============================================================================
// USAGE: linkmerge file1 file2 ...
// INPUT: a batch of link (text or gzip) file names from STDIN or @ARGV

// INPUT/OUTPUT format: CSV text
//         1.in 2.out 3.is_dest 4.star 5.delay 6.freq 7.ttl 8.monitor
//         1. the IP address of the ingress interface, e.g., 1.2.3.4
//         2. the IP address of the outgress interface, e.g., 5.6.7.8
//         3. whether the outgress node is the destination, e.g., Y or N
//         4. the number of anonymous (*) hops inbetween, e.g., 0 for directed link
//         5. the minimal delay in ms > 0, e.g., 10
//         6. the cumulative frequence of link observed, e.g., 5000
//         7. the minimal TTL of the ingress interface, e.g., 7
//         8. the monoitor which observed the link at the minimal TTL, e.g., 9.0.1.2

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

const NLINE: u32 = 100000;

// I/O helpers
fn openfile(path: &std::path::PathBuf) -> BufReader<Box<dyn Read>> {
    let input: Box<dyn std::io::Read + 'static> = if path.as_os_str() == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(&path).unwrap())
    };

    let reader: BufReader<Box<dyn Read>> = BufReader::new(input);

    return reader;
}

fn readfile(
    f: usize,
    file: &mut BufReader<Box<dyn std::io::Read>>,
    current_lines: &mut VecDeque<String>,
    lines_left: &mut Vec<u32>,
) -> u32 {
    let mut i = 0;
    while i < NLINE {
        let mut buf = String::new();
        if file.read_line(&mut buf).unwrap() == 0 {
            break;
        }
        let l = format!("{} {}", buf, f);
        current_lines.push_back(l);
        i += 1;
    }

    lines_left[f] = i;
    return i;
}

fn main() {
    let pargs = pico_args::Arguments::from_env();
    let mut inputs = pargs.finish();

    // remove duplicate filenames
    let mut h = HashMap::new();
    inputs.retain(|e| h.insert(String::from(e.to_str().unwrap()), true).is_none());

    // open all files
    let mut files: Vec<_> = inputs
        .iter()
        .map(|e| openfile(&PathBuf::from(&e)))
        .collect();

    let mut lines_left: Vec<u32> = inputs.iter().map(|_| 0).collect();
    let mut current_lines: VecDeque<String> = VecDeque::new();

    for (i, file) in files.iter_mut().enumerate() {
        readfile(i as usize, file, &mut current_lines, &mut lines_left);
    }
    current_lines.make_contiguous().sort();

    while current_lines.len() > 0 {
        let al = current_lines.pop_front().unwrap();
        let mut a: Vec<String> = al.split_whitespace().map(|x| x.to_string()).collect();
        let af = a.pop().unwrap().parse::<usize>().unwrap();
        lines_left[af] -= 1;

        if current_lines.len() <= 0 {
            println!("{}", a.join(" "));
            for line in BufReader::new(files[af].by_ref()).lines() {
                println!("{}", line.unwrap());
            }
            break;
        }

        let bl = &current_lines[0];
        let mut b: Vec<String> = bl.split_whitespace().map(|x| x.to_string()).collect();
        let bf = b.pop().unwrap().parse::<usize>().unwrap();

        if a[0] != b[0] || a[1] != b[1] {
            println!("{}", a.join(" "));
        } else {
            if a[2] == "N" {
                b[2] = "N".to_string();
            }
            if b[3].parse::<u32>().unwrap() > a[3].parse::<u32>().unwrap() {
                b[3] = a[3].to_string();
            }
            if b[4].parse::<f64>().unwrap() > a[4].parse::<f64>().unwrap() {
                b[4] = a[4].to_string();
            }
            b[5] = (b[5].parse::<u32>().unwrap() + a[5].parse::<u32>().unwrap()).to_string();
            let b6 = b[6].parse::<u32>().unwrap();
            let a6 = a[6].parse::<u32>().unwrap();
            if b6 > a6 || (b6 == a6 && a[7] < b[7]) {
                b[6] = a[6].to_string();
                b[7] = a[7].to_string();
            }

            b.push(String::from(bf.to_string()));
            current_lines[0] = b.join(" ");
        }

        if lines_left[af] == 0
        {
            let lines_read = readfile(af, &mut files[af], &mut current_lines, &mut lines_left);
            if lines_read != 0 {
                current_lines.make_contiguous().sort();
            }
        }
    }
}
