use std::io::{self, BufRead};
use std::net::Ipv4Addr;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut lines_iter = stdin.lock().lines();
    let mut p: Vec<String> = Vec::new();
    let mut ip: Ipv4Addr;
    let mut ip_lines: Vec<(u32, String)> = Vec::new();

    while let Some(Ok(line)) = lines_iter.next() {
        let l = line.trim().to_string();
        if !l.is_empty() && p.len() > 0 && l.chars().nth(0).unwrap_or_default() == 't' {
            ip = p[0].split_whitespace().nth(4).unwrap().parse().unwrap();
            ip_lines.push((u32::from(ip), p.join("|")));
            p.clear();
        }
        p.push(l);
    }

    if !p.is_empty() {
        ip = p[0].split_whitespace().nth(4).unwrap().parse().unwrap();
        ip_lines.push((u32::from(ip), p.join("|")));
    }

    ip_lines.sort_unstable_by_key(|k| k.0);

    for line in ip_lines {
        println!("{}", line.1);
    }

    Ok(())
}
