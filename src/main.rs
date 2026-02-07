use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;
use zpars::{CompressionOptions, DecompressionOptions};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "zpars",
    version,
    about = "Rust port of core ZPAQ LZ77 preprocessor codec"
)]
struct Cli {
    #[arg(long, default_value = "pretty")]
    log_format: LogFormat,

    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(long)]
    log_filter: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Compress(CompressArgs),
    Decompress(IoArgs),
    Roundtrip(CompressArgs),
    InspectZpaq(InspectArgs),
    ExtractZpaqM0(ExtractZpaqM0Args),
}

#[derive(Debug, Args)]
struct IoArgs {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct CompressArgs {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(long, default_value_t = 1 << 20)]
    block_size: usize,

    #[arg(long, default_value_t = 4)]
    min_match: usize,

    #[arg(long, default_value_t = 0)]
    secondary_match: usize,

    #[arg(long, default_value_t = 3)]
    search_log: u8,

    #[arg(long, default_value_t = 20)]
    table_log: u8,
}

#[derive(Debug, Args)]
struct InspectArgs {
    #[arg(short, long)]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct ExtractZpaqM0Args {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output_dir: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli)?;

    match cli.command {
        Command::Compress(args) => run_compress(&args),
        Command::Decompress(args) => run_decompress(&args),
        Command::Roundtrip(args) => run_roundtrip(&args),
        Command::InspectZpaq(args) => run_inspect_zpaq(&args),
        Command::ExtractZpaqM0(args) => run_extract_zpaq_m0(&args),
    }
}

fn run_compress(args: &CompressArgs) -> Result<()> {
    let opts = compression_options(args);
    info!(?opts, input = %args.input.display(), output = %args.output.display(), "compression started");

    let input = File::open(&args.input)
        .with_context(|| format!("opening input file {}", args.input.display()))?;
    let output = File::create(&args.output)
        .with_context(|| format!("creating output file {}", args.output.display()))?;

    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    zpars::compress(&mut reader, &mut writer, &opts)?;
    writer.flush()?;

    info!("compression completed");
    Ok(())
}

fn run_decompress(args: &IoArgs) -> Result<()> {
    info!(input = %args.input.display(), output = %args.output.display(), "decompression started");

    let input = File::open(&args.input)
        .with_context(|| format!("opening input file {}", args.input.display()))?;
    let output = File::create(&args.output)
        .with_context(|| format!("creating output file {}", args.output.display()))?;

    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    zpars::decompress(&mut reader, &mut writer, &DecompressionOptions)?;
    writer.flush()?;

    info!("decompression completed");
    Ok(())
}

fn run_roundtrip(args: &CompressArgs) -> Result<()> {
    let opts = compression_options(args);
    info!(input = %args.input.display(), output = %args.output.display(), "roundtrip started");

    let mut raw = Vec::new();
    File::open(&args.input)
        .with_context(|| format!("opening input file {}", args.input.display()))?
        .read_to_end(&mut raw)?;

    let mut compressed = Vec::new();
    zpars::compress(raw.as_slice(), &mut compressed, &opts)?;

    let mut restored = Vec::new();
    zpars::decompress(compressed.as_slice(), &mut restored, &DecompressionOptions)?;

    if raw != restored {
        anyhow::bail!("roundtrip mismatch");
    }

    let mut out = BufWriter::new(
        File::create(&args.output)
            .with_context(|| format!("creating output file {}", args.output.display()))?,
    );
    out.write_all(&restored)?;
    out.flush()?;

    debug!(
        raw = raw.len(),
        compressed = compressed.len(),
        restored = restored.len(),
        "roundtrip metrics"
    );
    info!("roundtrip completed");
    Ok(())
}

fn compression_options(args: &CompressArgs) -> CompressionOptions {
    CompressionOptions {
        block_size: args.block_size,
        min_match: args.min_match,
        secondary_match: args.secondary_match,
        search_log: args.search_log,
        table_log: args.table_log,
    }
}

fn init_tracing(cli: &Cli) -> Result<()> {
    let filter = if let Some(f) = &cli.log_filter {
        EnvFilter::new(f.clone())
    } else {
        let level = match cli.verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        };
        EnvFilter::new(level)
    };

    match cli.log_format {
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(true)
                .compact()
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(true)
                .json()
                .init();
        }
    }

    Ok(())
}

fn run_inspect_zpaq(args: &InspectArgs) -> Result<()> {
    let blocks = zpars::inspect_zpaq_file(&args.input)?;
    info!(count = blocks.len(), input = %args.input.display(), "zpaq blocks detected");
    for (idx, b) in blocks.iter().enumerate() {
        println!(
            "block={idx} offset={} level={} type={} hsize={} hh={} hm={} ph={} pm={} comps={} comp_bytes={} hcomp_bytes={} segment_offset={}",
            b.start_offset,
            b.level,
            b.zpaql_type,
            b.hsize,
            b.hh,
            b.hm,
            b.ph,
            b.pm,
            b.n_components,
            b.comp_bytes,
            b.hcomp_bytes,
            b.segment_offset
        );
    }
    Ok(())
}

fn run_extract_zpaq_m0(args: &ExtractZpaqM0Args) -> Result<()> {
    let segments = zpars::extract_zpaq_unmodeled_file(&args.input)?;
    std::fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "creating output directory for extracted files {}",
            args.output_dir.display()
        )
    })?;

    for seg in &segments {
        let name = if seg.filename.is_empty() {
            format!("block{}_segment.bin", seg.block_index)
        } else {
            seg.filename.clone()
        };
        let path = args.output_dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &seg.data)
            .with_context(|| format!("writing extracted file {}", path.display()))?;
        info!(
            block = seg.block_index,
            file = %path.display(),
            bytes = seg.data.len(),
            "extracted segment"
        );
    }

    info!(segments = segments.len(), "zpaq -m0 extraction completed");
    Ok(())
}
