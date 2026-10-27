//! Bounded raw DEFLATE reader for native ZIP members. No compressor or rewrite.
use anyhow::{bail, ensure, Result};

struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}
impl Bits<'_> {
    fn read(&mut self, n: usize) -> Result<u32> {
        ensure!(
            n <= 16 && self.bit + n <= self.data.len() * 8,
            "Truncated DEFLATE stream"
        );
        let mut out = 0;
        for i in 0..n {
            out |= (((self.data[self.bit / 8] >> (self.bit % 8)) & 1) as u32) << i;
            self.bit += 1;
        }
        Ok(out)
    }
}
struct Tree {
    codes: Vec<Vec<(u32, usize)>>,
}
impl Tree {
    fn new(lengths: &[u32]) -> Result<Self> {
        let mut counts = [0u32; 16];
        for &n in lengths {
            ensure!(n <= 15, "Invalid Huffman length");
            if n != 0 {
                counts[n as usize] += 1;
            }
        }
        let mut codes = vec![vec![]; 16];
        let mut next = [0u32; 16];
        let mut code = 0;
        for n in 1..16 {
            code = (code + counts[n - 1]) << 1;
            ensure!(code + counts[n] <= 1 << n, "Oversubscribed Huffman tree");
            next[n] = code;
        }
        for (symbol, &n) in lengths.iter().enumerate() {
            if n != 0 {
                let n = n as usize;
                codes[n].push((next[n], symbol));
                next[n] += 1;
            }
        }
        Ok(Self { codes })
    }
    fn symbol(&self, bits: &mut Bits<'_>) -> Result<usize> {
        // Codes arrive most-significant bit first, unlike numeric fields.
        let mut code = 0;
        for n in 1..16 {
            code = (code << 1) | bits.read(1)?;
            if let Ok(index) = self.codes[n].binary_search_by_key(&code, |(c, _)| *c) {
                return Ok(self.codes[n][index].1);
            }
        }
        bail!("Invalid Huffman code")
    }
}
fn dynamic(bits: &mut Bits<'_>) -> Result<(Tree, Tree)> {
    let literals = bits.read(5)? as usize + 257;
    let distances = bits.read(5)? as usize + 1;
    ensure!(literals <= 286, "Reserved literal count");
    let count = bits.read(4)? as usize + 4;
    let order = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let mut lengths = [0; 19];
    for &n in &order[..count] {
        lengths[n] = bits.read(3)?;
    }
    let tree = Tree::new(&lengths)?;
    let mut lengths = Vec::with_capacity(literals + distances);
    while lengths.len() < literals + distances {
        let symbol = tree.symbol(bits)?;
        let (length, repeat) = match symbol {
            0..=15 => (symbol as u32, 1),
            16 => (
                *lengths
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("Huffman repeat without predecessor"))?,
                bits.read(2)? as usize + 3,
            ),
            17 => (0, bits.read(3)? as usize + 3),
            18 => (0, bits.read(7)? as usize + 11),
            _ => bail!("Invalid code-length symbol"),
        };
        ensure!(
            lengths.len() + repeat <= literals + distances,
            "Huffman repeat overflow"
        );
        lengths.resize(lengths.len() + repeat, length);
    }
    ensure!(lengths[256] != 0, "Missing DEFLATE end code");
    Ok((
        Tree::new(&lengths[..literals])?,
        Tree::new(&lengths[literals..])?,
    ))
}

pub(super) fn inflate(data: &[u8], size: usize) -> Result<Vec<u8>> {
    ensure!(size <= 32 * 1024 * 1024, "Inflated native file too large");
    let mut bits = Bits { data, bit: 0 };
    let mut out = Vec::with_capacity(size);
    loop {
        let last = bits.read(1)? != 0;
        let kind = bits.read(2)?;
        if kind == 0 {
            bits.bit = bits.bit.div_ceil(8) * 8;
            let n = bits.read(16)? as usize;
            ensure!(
                (n as u32 ^ bits.read(16)?) == 65535,
                "Invalid uncompressed block length"
            );
            ensure!(out.len() + n <= size, "Inflated file exceeds declared size");
            for _ in 0..n {
                out.push(bits.read(8)? as u8);
            }
        } else {
            let (lit, dist) = match kind {
                1 => {
                    let mut lengths = vec![8; 288];
                    lengths[144..256].fill(9);
                    lengths[256..280].fill(7);
                    (Tree::new(&lengths)?, Tree::new(&[5; 32])?)
                }
                2 => dynamic(&mut bits)?,
                _ => bail!("Reserved DEFLATE block type"),
            };
            loop {
                match lit.symbol(&mut bits)? {
                    n @ 0..=255 => {
                        ensure!(out.len() < size, "Inflated file exceeds declared size");
                        out.push(n as u8);
                    }
                    256 => break,
                    n @ 257..=285 => {
                        const BASE: [usize; 29] = [
                            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51,
                            59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
                        ];
                        const EXTRA: [usize; 29] = [
                            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4,
                            5, 5, 5, 5, 0,
                        ];
                        let length = BASE[n - 257] + bits.read(EXTRA[n - 257])? as usize;
                        let d = dist.symbol(&mut bits)?;
                        ensure!(d < 30, "Reserved distance code");
                        let extra = if d < 4 { 0 } else { d / 2 - 1 };
                        let base = if d < 4 {
                            d + 1
                        } else {
                            ((2 + d % 2) << extra) + 1
                        };
                        let distance = base + bits.read(extra)? as usize;
                        ensure!(
                            distance <= out.len() && out.len() + length <= size,
                            "Invalid DEFLATE backreference"
                        );
                        for _ in 0..length {
                            out.push(out[out.len() - distance]);
                        }
                    }
                    _ => bail!("Reserved literal code"),
                }
            }
        }
        if last {
            break;
        }
    }
    ensure!(
        out.len() == size && bits.bit.div_ceil(8) == data.len(),
        "DEFLATE size/trailing data mismatch"
    );
    Ok(out)
}
