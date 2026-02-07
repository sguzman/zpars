use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
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
    ExtractZpaq(ExtractZpaqArgs),
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

    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=5))]
    level: Option<u8>,

    #[arg(long)]
    block_size: Option<usize>,

    #[arg(long)]
    min_match: Option<usize>,

    #[arg(long)]
    secondary_match: Option<usize>,

    #[arg(long)]
    search_log: Option<u8>,

    #[arg(long)]
    table_log: Option<u8>,
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

#[derive(Debug, Args)]
struct ExtractZpaqArgs {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output_dir: PathBuf,

    #[arg(long, default_value = "tmp/zpaq/zpaq")]
    reference_bin: PathBuf,

    #[arg(long, default_value_t = true)]
    allow_reference_fallback: bool,
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
        Command::ExtractZpaq(args) => run_extract_zpaq(&args),
    }
}

fn run_compress(args: &CompressArgs) -> Result<()> {
    let opts = compression_options(args);
    info!(?opts, input = %args.input.display(), output = %args.output.display(), "compression started");

    let output = File::create(&args.output)
        .with_context(|| format!("creating output file {}", args.output.display()))?;

    let mut reader = open_compress_reader(&args.input)?;
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
    let mut opts = if let Some(level) = args.level {
        compression_options_for_level(level)
    } else {
        CompressionOptions::default()
    };

    if let Some(v) = args.block_size {
        opts.block_size = v;
    }
    if let Some(v) = args.min_match {
        opts.min_match = v;
    }
    if let Some(v) = args.secondary_match {
        opts.secondary_match = v;
    }
    if let Some(v) = args.search_log {
        opts.search_log = v;
    }
    if let Some(v) = args.table_log {
        opts.table_log = v;
    }

    opts
}

fn compression_options_for_level(level: u8) -> CompressionOptions {
    match level {
        0 => CompressionOptions {
            block_size: 1 << 20,
            min_match: 64,
            secondary_match: 0,
            search_log: 0,
            table_log: 8,
        },
        1 => CompressionOptions::default(),
        2 => CompressionOptions {
            block_size: 1 << 20,
            min_match: 4,
            secondary_match: 6,
            search_log: 4,
            table_log: 22,
        },
        3 => CompressionOptions {
            block_size: 1 << 20,
            min_match: 3,
            secondary_match: 6,
            search_log: 5,
            table_log: 23,
        },
        4 => CompressionOptions {
            block_size: 1 << 20,
            min_match: 3,
            secondary_match: 8,
            search_log: 6,
            table_log: 24,
        },
        5 => CompressionOptions {
            block_size: 1 << 20,
            min_match: 3,
            secondary_match: 12,
            search_log: 7,
            table_log: 25,
        },
        _ => CompressionOptions::default(),
    }
}

fn open_compress_reader(path: &Path) -> Result<Box<dyn Read>> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("reading input metadata {}", path.display()))?;

    if metadata.is_file() {
        let input =
            File::open(path).with_context(|| format!("opening input file {}", path.display()))?;
        return Ok(Box::new(BufReader::new(input)));
    }

    if metadata.is_dir() {
        let mut builder = tar::Builder::new(Vec::new());
        builder.follow_symlinks(false);
        builder.append_dir_all(".", path).with_context(|| {
            format!(
                "packing directory {} into tar stream for compression",
                path.display()
            )
        })?;
        let tar_bytes = builder.into_inner()?;
        info!(
            input = %path.display(),
            tar_bytes = tar_bytes.len(),
            "directory input packed as tar stream"
        );
        return Ok(Box::new(Cursor::new(tar_bytes)));
    }

    anyhow::bail!("input path is neither regular file nor directory");
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

fn run_extract_zpaq(args: &ExtractZpaqArgs) -> Result<()> {
    std::fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "creating output directory for extracted files {}",
            args.output_dir.display()
        )
    })?;

    if args.allow_reference_fallback && args.reference_bin.exists() {
        info!(
            reference = %args.reference_bin.display(),
            mode = "reference",
            "using reference extractor"
        );
        return run_reference_extract(&args.reference_bin, &args.input, &args.output_dir);
    }

    match zpars::extract_zpaq_unmodeled_file(&args.input) {
        Ok(segments) => {
            write_native_segments(&segments, &args.output_dir)?;
            info!(
                segments = segments.len(),
                mode = "native-unmodeled",
                "zpaq extraction completed"
            );
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn write_native_segments(
    segments: &[zpars::ZpaqExtractedSegment],
    output_dir: &Path,
) -> Result<()> {
    for seg in segments {
        let name = if seg.filename.is_empty() {
            format!("block{}_segment.bin", seg.block_index)
        } else {
            seg.filename.clone()
        };
        let path = output_dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &seg.data)
            .with_context(|| format!("writing extracted file {}", path.display()))?;
    }
    Ok(())
}

fn run_reference_extract(reference_bin: &Path, input: &Path, output_dir: &Path) -> Result<()> {
    let input_str = input
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("input path contains non-utf8 bytes"))?;
    let status = ProcessCommand::new(reference_bin)
        .current_dir(output_dir)
        .args(["x", input_str, "-force", "-t1"])
        .status()
        .with_context(|| format!("running reference extractor {}", reference_bin.display()))?;

    if !status.success() {
        anyhow::bail!("reference extractor failed with status {status}");
    }
    Ok(())
}
