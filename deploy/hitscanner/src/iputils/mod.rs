// Utilities for geopt, based on https://github.com/NLnetLabs/try-tries-and-trees
//   - IPRange: useful for parsing geodb raw data
//     - implements from into trait for Vec<Prefix>
//   - PrefixGeo: prefix meta data that holds
//     - 2 letters country code (u16)

use std::fmt::Debug;
use std::{
    convert::{From, TryFrom},
    fs::File,
    io::{BufRead, BufReader},
    marker::PhantomData,
    net::Ipv4Addr,
    path::PathBuf,
};
use trie::common::{NoMeta, Prefix, Trie};

// ISO 3166-1 alpha-2: country code
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CountryCodeAlpha2(u16);
#[allow(dead_code)]
pub struct PrefixGeo {
    country: CountryCodeAlpha2,
}

impl From<CountryCodeAlpha2> for String {
    fn from(country_code: CountryCodeAlpha2) -> Self {
        let bytes = [(country_code.0 >> 8) as u8, (country_code.0 & 0xFF) as u8];
        String::from_utf8_lossy(&bytes).to_string()
    }
}

impl TryFrom<&str> for CountryCodeAlpha2 {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() != 2 {
            return Err("Country code must have exactly 2 letters");
        }
        let bytes = value.as_bytes();
        let combined = ((bytes[0] as u16) << 8) | (bytes[1] as u16);
        Ok(CountryCodeAlpha2(combined))
    }
}

// can't patch Prefix since it's in another crate
pub fn parse_prefix_str(ps: &str) -> Prefix<u32, NoMeta> {
    let mut p = ps.split("/");
    let ip: Vec<_> = p
        .next()
        .unwrap()
        .split(".")
        .map(|o| -> u8 { o.parse().unwrap() })
        .collect();
    let net: u32 = Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]).into();
    let len: u8 = p.next().unwrap().parse().unwrap();
    // make sure the last $len bits are zeros
    Prefix::<u32, NoMeta>::new(net >> (32-len) << (32-len), len)
}

// Inclusive range [a, b]
pub struct IPRange {
    pub a: u32,
    pub b: u32,
}

fn r2c_helper(a: u64, b: u64, l: u64, h: u64) -> Vec<(u64, u64)> {
    if (a, b) == (l, h) {
        return vec![(l, h)];
    }
    let m = (h + l) / 2;
    if b <= m {
        r2c_helper(a, b, l, m)
    } else if a >= m + 1 {
        r2c_helper(a, b, m + 1, h)
    } else {
        let mut al = r2c_helper(a, m, l, m);
        let mut bl = r2c_helper(m + 1, b, m + 1, h);
        al.append(&mut bl);
        al
    }
}

fn iprange2_prefix(a: u64, b: u64) -> Vec<Prefix<u32, NoMeta>> {
    let (l, h) = (0, 0xFF_FF_FF_FFu64);
    r2c_helper(a, b, l, h)
        .into_iter()
        .map(|(start, end)| {
            let prefix_length = 32 - ((end - start + 1) as f64).log2() as u8;
            Prefix::<u32, NoMeta>::new(start as u32, prefix_length)
        })
        .collect()
}

/*
IPRange到Prefix的转换算法
- 输入：若干`iprange`，注意是闭区间
- 输出：`iprange`对应的cidr表示
- 伪代码：
# 二分递归函数
def r2c_helper(a, b, l, h):
  if (a,b) == (l,h):
    return [(l,h)]
  m = (h+l)/2
  if b <= m:
    return r2c_helper(a, b, l, m)
  elif a >= m+1:
    return r2c_helper(a, b, m+1, h)
  else:
    return r2c_helper(a, m, l, m) + r2c_helper(m+1, b, m+1, h)

# 必然能通过二分找到对齐区间两个端点a,b的前缀。
# l, h表示包含a, b的，且为2的整数倍的，上下界。
# 初始为0~2^32-1，通过二分逐步缩小。
def range2cidr(a, b):
  l, h = 0, 2**32-1
  return map( lambda x: int2ip(x[0])+'/'+str(32-int(math.log(x[1]-x[0]+1, 2))), r2c_helper(a, b, l, h)
*/
impl Into<Vec<Prefix<u32, NoMeta>>> for IPRange {
    fn into(self) -> Vec<Prefix<u32, NoMeta>> {
        iprange2_prefix(self.a as u64, self.b as u64)
    }
}

// IP Labeller
#[allow(dead_code)]
pub struct IPLabeller<'a, T: ProcessLine>(
    pub Trie<'a, u32, String>,
    &'a Vec<Prefix<u32, String>>,
    PhantomData<T>,
);

pub trait ProcessLine {
    fn process_line(line: &String) -> Vec<Prefix<u32, String>>;
}

impl ProcessLine for IPRange {
    fn process_line(line: &String) -> Vec<Prefix<u32, String>> {
        let mut fields = line.split(',');
        let r = IPRange {
            a: fields.next().unwrap().parse().unwrap(),
            b: fields.next().unwrap().parse().unwrap(),
        };
        let g: String = fields.collect::<Vec<_>>().join(",");
        let pv: Vec<Prefix<u32, NoMeta>> = r.into();
        let mut pfxs: Vec<_> = vec![];
        for p in pv {
            pfxs.push(Prefix::new_with_meta(p.net, p.len, g.clone()));
        }
        pfxs
    }
}

impl ProcessLine for Prefix<u32, String> {
    fn process_line(line: &String) -> Vec<Prefix<u32, String>> {
        let mut fields = line.split(' ');
        let r1 = fields.next().unwrap();
        let pfx = parse_prefix_str(&r1);
        let g = fields.next().unwrap().to_string();
        vec![Prefix::new_with_meta(pfx.net, pfx.len, g)]
    }
}

#[allow(dead_code)]
impl<'a, T: ProcessLine> IPLabeller<'a, T> {
    pub fn new(path: &PathBuf, pfxs: &'a mut Vec<Prefix<u32, String>>) -> Self {
        let mut trie = Trie::<u32, String>::new();
        for line in BufReader::new(File::open(&path).unwrap()).lines() {
            for p in <T as ProcessLine>::process_line(line.as_ref().unwrap()) {
                pfxs.push(p);
            }
        }
        for pfx in pfxs.iter() {
            trie.insert(pfx);
        }
        IPLabeller(trie, pfxs, PhantomData)
    }

    pub fn match_pfx(
        &self,
        pfx: &Prefix<u32, NoMeta>,
    ) -> Option<&trie::common::Prefix<u32, String>> {
        self.0.match_longest_prefix(pfx)
    }
}
