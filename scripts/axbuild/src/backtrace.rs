use std::{fs, io::Read, path::PathBuf};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use regex::Regex;

#[derive(Subcommand)]
pub enum Command {
    /// Extract and symbolize BACKTRACE_BEGIN/BT/BACKTRACE_END blocks from text logs.
    Symbolize(SymbolizeArgs),
}

#[derive(Args)]
pub struct SymbolizeArgs {
    /// Path to the kernel/app ELF file to symbolize addresses against.
    #[arg(long, value_name = "PATH")]
    pub elf: PathBuf,

    /// Path to the captured log. If omitted, read from stdin.
    #[arg(long, value_name = "PATH")]
    pub log: Option<PathBuf>,

    /// Only symbolize blocks whose kind matches this value.
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,

    /// Subtract 1 from ip before symbolization (matches typical call-site adjustment).
    #[arg(long, default_value_t = true)]
    pub adjust_ip: bool,
}

pub fn execute(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Symbolize(args) => symbolize(args),
    }
}

#[derive(Debug, Clone)]
struct Frame {
    idx: usize,
    ip: u64,
    fp: u64,
}

#[derive(Debug, Clone)]
struct Block {
    kind: String,
    arch: Option<String>,
    frames: Vec<Frame>,
}

fn read_text(log: Option<PathBuf>) -> anyhow::Result<String> {
    match log {
        Some(path) => Ok(fs::read_to_string(&path)
            .with_context(|| format!("failed to read log {}", path.display()))?),
        None => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .context("failed to read stdin")?;
            Ok(s)
        }
    }
}

fn parse_blocks(text: &str) -> anyhow::Result<Vec<Block>> {
    let begin_re = Regex::new(r"BACKTRACE_BEGIN\b.*\bkind=([^\s]+)\b(?:.*\barch=([^\s]+)\b)?")
        .context("invalid begin regex")?;
    let frame_re =
        Regex::new(r"\bBT\s+(\d+)\s+ip=0x([0-9a-fA-F]+)\s+fp=0x([0-9a-fA-F]+)")
            .context("invalid frame regex")?;
    let end_re = Regex::new(r"BACKTRACE_END\b").context("invalid end regex")?;

    #[derive(Debug)]
    enum State {
        Idle,
        Capturing(Block),
    }

    let mut state = State::Idle;
    let mut out = Vec::new();

    for line in text.lines() {
        match &mut state {
            State::Idle => {
                if let Some(cap) = begin_re.captures(line) {
                    let kind = cap.get(1).unwrap().as_str().to_string();
                    let arch = cap.get(2).map(|m| m.as_str().to_string());
                    state = State::Capturing(Block {
                        kind,
                        arch,
                        frames: Vec::new(),
                    });
                }
            }
            State::Capturing(block) => {
                if end_re.is_match(line) {
                    out.push(block.clone());
                    state = State::Idle;
                    continue;
                }

                if let Some(cap) = frame_re.captures(line) {
                    let idx: usize = cap.get(1).unwrap().as_str().parse()?;
                    let ip = u64::from_str_radix(cap.get(2).unwrap().as_str(), 16)?;
                    let fp = u64::from_str_radix(cap.get(3).unwrap().as_str(), 16)?;
                    block.frames.push(Frame { idx, ip, fp });
                }
            }
        }
    }

    Ok(out)
}

fn symbolize(args: SymbolizeArgs) -> anyhow::Result<()> {
    let text = read_text(args.log)?;
    let blocks = parse_blocks(&text)?;
    if blocks.is_empty() {
        bail!("no backtrace blocks found");
    }

    let loader = addr2line::Loader::new(&args.elf).map_err(|err| {
        anyhow::anyhow!(
            "failed to load dwarf/symbols from {}: {}",
            args.elf.display(),
            err
        )
    })?;

    for (i, block) in blocks.iter().enumerate() {
        if let Some(kind) = &args.kind {
            if &block.kind != kind {
                continue;
            }
        }

        println!(
            "BACKTRACE_BLOCK {} kind={} arch={}",
            i,
            block.kind,
            block.arch.as_deref().unwrap_or("?")
        );

        for frame in &block.frames {
            let ip = if args.adjust_ip && frame.ip > 0 {
                frame.ip - 1
            } else {
                frame.ip
            };
            let symbolized = symbolize_with_loader(&loader, ip);

            match symbolized {
                Some(sym) => {
                    println!("BT {} ip=0x{:x} fp=0x{:x} {}", frame.idx, frame.ip, frame.fp, sym);
                }
                None => {
                    println!("BT {} ip=0x{:x} fp=0x{:x}", frame.idx, frame.ip, frame.fp);
                }
            }
        }
    }

    Ok(())
}

fn symbolize_with_loader(loader: &addr2line::Loader, ip: u64) -> Option<String> {
    let mut frames = loader.find_frames(ip).ok()?;
    let mut out = Vec::new();
    while let Some(frame) = frames.next().ok()? {
        let name = frame
            .function
            .as_ref()
            .and_then(|f| f.raw_name().ok())
            .map(|s| rustc_demangle::demangle(s.as_ref()).to_string());
        let loc = frame.location.as_ref().and_then(|l| {
            let file = l.file?;
            let line = l.line?;
            Some(format!("{file}:{line}"))
        });
        match (name, loc) {
            (Some(name), Some(loc)) => out.push(format!("{name} ({loc})")),
            (Some(name), None) => out.push(name),
            (None, Some(loc)) => out.push(loc),
            (None, None) => {}
        }
    }
    if out.is_empty() {
        let sym = loader.find_symbol(ip).map(|s| rustc_demangle::demangle(s).to_string());
        return sym;
    }

    Some(out.join(" ; "))
}
