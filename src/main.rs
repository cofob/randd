use clap::Parser;
use rand::Rng;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
enum StatusLevel {
    None,
    Noxfer,
    Progress,
    BitArray,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'i', long = "if")]
    input: Option<String>,

    #[arg(short = 'o', long = "of")]
    output: Option<String>,

    #[arg(short = 'b', long)]
    bs: String,

    #[arg(long)]
    count: Option<NonZeroU64>,

    #[arg(long)]
    skip: Option<u64>,

    #[arg(long)]
    seek: Option<u64>,

    #[arg(long)]
    speed: Option<String>,

    #[arg(short = 's', long, value_delimiter = ',', use_value_delimiter = true)]
    conv: Vec<String>,

    #[arg(long)]
    status: Option<String>,
}

struct RandomDd {
    args: Args,
    bs_min: u64,
    bs_max: u64,
    noerror: bool,
    sync: bool,
    status_level: StatusLevel,
    bytes_copied: AtomicU64,
    start_time: Instant,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
impl RandomDd {
    fn new(args: Args) -> Result<Self, String> {
        let (bs_min, bs_max) = Self::parse_bs_range(&args.bs)?;

        let noerror = args.conv.iter().any(|c| c == "noerror");
        let sync = args.conv.iter().any(|c| c == "sync");

        let status_level = match args.status.as_deref() {
            Some("none") => StatusLevel::None,
            Some("progress") => StatusLevel::Progress,
            Some("bitarray") => StatusLevel::BitArray,
            Some("noxfer") | None => StatusLevel::Noxfer,
            Some(s) => return Err(format!("Invalid status value: {s}")),
        };

        Ok(Self {
            args,
            bs_min,
            bs_max,
            noerror,
            sync,
            status_level,
            bytes_copied: AtomicU64::new(0),
            start_time: Instant::now(),
        })
    }

    fn parse_size(s: &str) -> Result<u64, String> {
        let s = s.trim().to_lowercase();
        let (num, suffix) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));

        let base: u64 = num.parse().map_err(|e| format!("Invalid size: {e}"))?;

        Ok(match suffix {
            "b" => base * 512,
            "k" => base * 1024,
            "m" => base * 1024 * 1024,
            "g" => base * 1024 * 1024 * 1024,
            "t" => base * 1024 * 1024 * 1024 * 1024,
            "p" => base * 1024 * 1024 * 1024 * 1024 * 1024,
            "w" => base * 4,
            "" => base,
            _ => return Err(format!("Unknown size suffix: {suffix}")),
        })
    }

    fn parse_bs_range(s: &str) -> Result<(u64, u64), String> {
        if let Some((min, max)) = s.split_once('-') {
            let min_size = Self::parse_size(min)?;
            let max_size = Self::parse_size(max)?;

            if min_size > max_size {
                return Err(format!(
                    "Invalid block size range: min ({min}) > max ({max})"
                ));
            }

            Ok((min_size, max_size))
        } else {
            let size = Self::parse_size(s)?;
            Ok((size, size))
        }
    }

    fn format_size(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_idx])
    }

    fn format_speed(bytes_per_sec: f64) -> String {
        Self::format_size(bytes_per_sec as u64) + "/s"
    }

    fn flip_bit(bitarray: &Arc<Mutex<Vec<u8>>>, index: u64) {
        let mut ba = bitarray.lock().unwrap();
        let byte_idx = (index / 8) as usize;
        let bit_idx = (index % 8) as usize;
        if byte_idx < ba.len() {
            ba[byte_idx] ^= 1 << bit_idx;
        }
    }

    fn visualize_bitarray(bitarray: &Vec<u8>, width: usize) -> String {
        const LINE_WIDTH: usize = 64;
        let mut result = String::new();
        let mut count = 0;
        for byte in bitarray {
            for bit in 0..8 {
                if count >= width {
                    break;
                }
                result.push(if (byte >> bit) & 1 == 1 { '#' } else { '.' });
                count += 1;
                if count % LINE_WIDTH == 0 {
                    result.push('\n');
                }
            }
        }
        result
    }

    fn start_progress_thread(
        &self,
        bitarray: &Arc<Mutex<Vec<u8>>>,
        bitarray_size: u64,
    ) -> Option<JoinHandle<()>> {
        if self.status_level != StatusLevel::Progress && self.status_level != StatusLevel::BitArray
        {
            return None;
        }

        let bytes_copied = &raw const self.bytes_copied as u64;
        let start_time = self.start_time;
        let bitarray_clone = Arc::clone(bitarray);
        let bitarray_display_size = if bitarray_size <= 512 {
            bitarray_size as usize
        } else {
            512
        };

        Some(thread::spawn(move || {
            let bytes_copied = unsafe { &*(bytes_copied as *const AtomicU64) };
            let is_bitarray = !bitarray_clone.lock().unwrap().is_empty();

            loop {
                thread::sleep(Duration::from_secs(1));

                let bytes = bytes_copied.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes as f64 / elapsed
                } else {
                    0.0
                };

                if is_bitarray {
                    let ba = bitarray_clone.lock().unwrap();
                    let visualization = Self::visualize_bitarray(&ba, bitarray_display_size);
                    eprintln!("\r{visualization}");
                    eprintln!(
                        "{}, {}",
                        Self::format_size(bytes),
                        Self::format_speed(speed)
                    );
                } else {
                    eprint!(
                        "\r{}, {}",
                        Self::format_size(bytes),
                        Self::format_speed(speed)
                    );
                    eprint!("\x1b[0K");
                }
            }
        }))
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn run(&self) -> Result<(), String> {
        let input_path = self.args.input.as_deref().unwrap_or("/dev/stdin");
        let output_path = self.args.output.as_deref().unwrap_or("/dev/stdout");

        let mut input = File::open(input_path)
            .map_err(|e| format!("Failed to open input {input_path:?}: {e}"))?;

        let mut output = File::options()
            .write(true)
            .open(output_path)
            .map_err(|e| format!("Failed to open output {output_path:?} (file must exist): {e}"))?;

        if let Some(skip) = self.args.skip {
            let block_size = self.bs_min.max(self.bs_max);
            input
                .seek(SeekFrom::Start(skip * block_size))
                .map_err(|e| format!("Failed to seek input: {e}"))?;
        }

        let output_size = output
            .metadata()
            .map_err(|e| format!("Failed to get output file metadata: {e}"))?
            .len();

        if output_size == 0 {
            return Err(format!(
                "Output file {output_path:?} has zero size, cannot write to random positions"
            ));
        }

        if self.bs_min > output_size {
            return Err(format!(
                "Block size ({}) is larger than output file size ({}), cannot write to random positions",
                Self::format_size(self.bs_min),
                Self::format_size(output_size)
            ));
        }

        let max_blocks = self.args.count.map(std::num::NonZero::get);
        let speed_limit = self
            .args
            .speed
            .as_ref()
            .map(|s| Self::parse_size(s))
            .transpose()
            .map_err(|e| format!("Failed to parse speed: {e}"))?;

        let bitarray: Arc<Mutex<Vec<u8>>> = if self.status_level == StatusLevel::BitArray {
            let bitarray_size = output_size.div_ceil(self.bs_min);
            let byte_count = bitarray_size.div_ceil(8) as usize;
            eprintln!("Bitarray size: {bitarray_size} bits ({byte_count} bytes)");
            Arc::new(Mutex::new(vec![0u8; byte_count]))
        } else {
            Arc::new(Mutex::new(vec![]))
        };

        let bitarray_size = output_size.div_ceil(self.bs_min);

        let progress_thread = self.start_progress_thread(&bitarray, bitarray_size);
        let mut blocks_processed: u64 = 0;

        let mut rng = rand::thread_rng();
        let mut last_report = Instant::now();

        loop {
            if let Some(max) = max_blocks {
                if blocks_processed >= max {
                    break;
                }
            }

            let chunk_size = if self.bs_min == self.bs_max {
                self.bs_min
            } else {
                rng.gen_range(self.bs_min..=self.bs_max)
            };

            let mut buffer = vec![0u8; chunk_size as usize];

            let read_result = input.read_exact(&mut buffer);

            let actual_read = match read_result {
                Ok(()) => chunk_size,
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    let bytes_read = input
                        .read(&mut buffer)
                        .map_err(|e| format!("Failed to read from input: {e}"))?;

                    if bytes_read == 0 {
                        break;
                    }

                    if self.sync {
                        for byte in &mut buffer[bytes_read..] {
                            *byte = 0;
                        }
                    }

                    bytes_read as u64
                }
                Err(e) if self.noerror => {
                    eprintln!("Input error (continuing): {e}");

                    if self.sync {
                        buffer.fill(0);
                        chunk_size
                    } else {
                        let current_pos = input
                            .stream_position()
                            .map_err(|e| format!("Failed to get input position: {e}"))?;
                        input
                            .seek(SeekFrom::Start(current_pos + chunk_size))
                            .map_err(|e| format!("Failed to seek past error: {e}"))?;
                        continue;
                    }
                }
                Err(e) => return Err(format!("Input error: {e}")),
            };

            let output_pos = rng.gen_range(0..=(output_size.saturating_sub(actual_read)));

            output
                .seek(SeekFrom::Start(output_pos))
                .map_err(|e| format!("Failed to seek output to {output_pos}: {e}"))?;

            if let Err(e) = output.write_all(&buffer[..actual_read as usize]) {
                if self.noerror {
                    eprintln!("Output error (continuing): {e}");
                } else {
                    return Err(format!("Output error: {e}"));
                }
            }

            if self.status_level == StatusLevel::BitArray {
                let bit_index = output_pos / self.bs_min;
                Self::flip_bit(&bitarray, bit_index);
            }

            if let Err(e) = output.flush() {
                if self.noerror {
                    eprintln!("Flush error: {e}");
                } else {
                    return Err(format!("Flush error: {e}"));
                }
            }

            blocks_processed += 1;
            self.bytes_copied.fetch_add(actual_read, Ordering::Relaxed);

            if self.status_level != StatusLevel::None
                && self.status_level != StatusLevel::Progress
                && self.status_level != StatusLevel::BitArray
                && last_report.elapsed() >= Duration::from_millis(100)
            {
                let bytes = self.bytes_copied.load(Ordering::Relaxed);
                eprint!("\r{}", Self::format_size(bytes));
                last_report = Instant::now();
            }

            if let Some(speed) = speed_limit {
                let elapsed = self.start_time.elapsed().as_secs_f64();
                let expected = (blocks_processed as f64) * chunk_size as f64 / speed as f64;
                if expected > elapsed {
                    thread::sleep(Duration::from_secs_f64(expected - elapsed));
                }
            }
        }

        if let Some(handle) = progress_thread {
            std::mem::forget(handle);
        }

        if self.status_level != StatusLevel::None {
            let bytes = self.bytes_copied.load(Ordering::Relaxed);
            let elapsed = self.start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                bytes as f64 / elapsed
            } else {
                0.0
            };

            if self.status_level != StatusLevel::Progress
                && self.status_level != StatusLevel::BitArray
            {
                eprintln!();
            }

            if self.status_level == StatusLevel::BitArray {
                eprintln!();
                let ba = bitarray.lock().unwrap();
                let bitarray_display_size = if bitarray_size <= 512 {
                    bitarray_size as usize
                } else {
                    512
                };
                let visualization = Self::visualize_bitarray(&ba, bitarray_display_size);
                eprintln!("Final bitarray state:");
                eprintln!("{visualization}");
                eprintln!();
            }

            eprintln!(
                "{} copied, {}, {} blocks",
                Self::format_size(bytes),
                Self::format_speed(speed),
                blocks_processed
            );
        }

        Ok(())
    }
}

fn main() {
    let args = Args::parse();

    match RandomDd::new(args) {
        Ok(dd) => {
            if let Err(e) = dd.run() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
