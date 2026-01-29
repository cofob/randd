# randd

A command-line utility similar to `dd(1)` that writes to random positions in files and block devices with variable chunk sizes. Unlike `dd`, `randd` writes data to random positions within the output file/device rather than sequentially.

## Key Differences from `dd`

| Feature | `dd` | `randd` |
|---------|------|---------|
| Write pattern | Sequential | Random positions |
| Block size | Fixed | Fixed or range |
| File size | Can resize | Never resizes |
| Visualization | Progress only | Progress + bitarray visualization |

## Features

- Read from input file/device and write to random positions in output file/device
- Specify block size as a range (e.g., `--bs 8M-32M`) or fixed size (e.g., `--bs 16M`)
- **Output file size never changes** - writes only within existing boundaries
- **Bitarray visualization** - see which parts of the file have been written
- Support for most sensible `dd` flags:
  - `--if FILE` or `-i FILE` - Input file (default: stdin)
  - `--of FILE` or `-o FILE` - Output file (default: stdout, must exist)
  - `--bs` or `-b` - Block size, supports range format `MIN-MAX`
  - `--count N` - Number of blocks to copy
  - `--skip N` - Skip N input blocks
  - `--seek N` - Seek N output blocks before starting
  - `--speed N` - Limit copy speed, supports size suffixes (e.g., `100M`)
  - `--conv noerror,sync` or `-s noerror,sync` - Conversion options
  - `--status progress|bitarray|noxfer|none` - Status reporting mode

## Quick Start

```bash
# Create a 1GB output file
dd if=/dev/zero of=output.bin bs=1M count=1000

# Write random chunks to random positions
randd --if /dev/urandom --of output.bin --bs 1M-8M --count 50 --status bitarray
```

## Usage Examples

### 1. Basic random writing with progress

```bash
randd --if /dev/urandom --of output.bin --bs 4M --count 100 --status progress
```

### 2. Variable block sizes with bitarray visualization

```bash
# Block sizes between 8MB and 32MB, randomly selected for each chunk
randd --if /dev/urandom --of output.bin --bs 8M-32M --count 20 --status bitarray
```

Output:
```
Bitarray size: 128 bits (16 bytes)

.#.##.#..#......#.#..#....#.........#..#....#.....##.#........#.
..###..#.......###.....##.#...#.#..#..........#......#.#....##..
#.#.........#...#####...#..#......##.##..#.............#..#..#.#
...#.##.......#.#..........#.#.#....#..........#...#..#.#....#.

320.00 MB copied, 1.85 GB/s, 20 blocks
```

### 3. Speed-limited writing

```bash
# Limit to 100 MB/s
randd --if /dev/urandom --of output.bin --bs 16M --count 50 --speed 100M --status progress
```

### 4. Error handling with noerror and sync

```bash
# Continue on input errors, pad with zeros
randd --if /dev/urandom --of output.bin --bs 1M --count 100 \
  --conv noerror,sync --status bitarray
```

### 5. Writing to a block device (DESTRUCTIVE)

```bash
# WARNING: This will corrupt the target device!
randd --if /dev/urandom --of /dev/sdb --bs 16M --status bitarray
```

## Bitarray Visualization

The `--status bitarray` mode provides a visual representation of which parts of the output file have been written:

- **`.` (dot)** - Block not yet written (bit = 0)
- **`#` (hash)** - Block has been written (bit = 1)
- Bits flip each time a block is written to that position
- Bit array size = ⌈output_file_size / minimum_block_size⌉

**Example:**
```
Bitarray size: 256 bits (32 bytes)

.#.##.#..#......#.#..#....#.........#..#....#.....##.#........#.
..###..#.......###.....##.#...#.#..#..........#......#.#....##..
#.#.........#...#####...#..#......##.##..#.............#..#..#.#
...#.##.......#.#..........#.#.#....#..........#...#..#.#....#.
```

This shows that blocks at positions 1, 3, 4, 6, etc. (0-indexed) have been written.

## Size Suffixes

All size parameters (`--bs`, `--speed`) support the following suffixes:

| Suffix | Multiplier | Size | Example |
|--------|------------|------|---------|
| `b` | 512 | 512 bytes | `1k` |
| `k` | 1024 | 1 KiB | `256k` |
| `m` | 1,048,576 | 1 MiB | `8m` |
| `g` | 1,073,741,824 | 1 GiB | `2g` |
| `t` | 1,099,511,627,776 | 1 TiB | `1t` |
| `p` | 1,125,899,906,842,624 | 1 PiB | `1p` |
| `w` | 4 | 4 bytes | `1024w` |

## Status Modes

| Mode | Description |
|------|-------------|
| `progress` | Shows bytes copied and speed, updates every second |
| `bitarray` | Shows visualization of written blocks, updates in real-time |
| `noxfer` | Hides transfer statistics, shows final summary |
| `none` | Hides all status output (errors still shown) |

## Important Constraints

### Output File Requirements

**The output file MUST exist before running `randd`.** It will never be created or resized.

```bash
# ✅ Correct: File exists
dd if=/dev/zero of=output.bin bs=1M count=1000
randd --if /dev/urandom --of output.bin --bs 1M --count 100

# ❌ Error: File doesn't exist
randd --if /dev/urandom --of output.bin --bs 1M --count 100
# Error: Failed to open output "output.bin" (file must exist)

# ❌ Error: File is empty
touch output.bin
randd --if /dev/urandom --of output.bin --bs 1M --count 100
# Error: Output file has zero size, cannot write to random positions
```

### Block Size Constraints

- The minimum block size must be smaller than the output file size
- For variable block sizes (`1M-8M`), the minimum (`1M`) is used for bitarray calculations

```bash
# ✅ Correct: Block size < file size
dd if=/dev/zero of=output.bin bs=1M count=100  # 100MB file
randd --if /dev/urandom --of output.bin --bs 1M --count 10   # 1MB < 100MB

# ❌ Error: Block size > file size
dd if=/dev/zero of=output.bin bs=1 count=1000  # 1KB file
randd --if /dev/urandom --of output.bin --bs 1M --count 10   # 1MB > 1KB
# Error: Block size (1.00 MB) is larger than output file size (1000.00 B)
```

### File Size Preservation

The output file size never changes, regardless of how much data is written:

```bash
dd if=/dev/zero of=output.bin bs=1M count=100  # Create 100MB file
BEFORE=$(stat -f%z output.bin)                 # 104857600 bytes
randd --if /dev/urandom --of output.bin --bs 1M --count 200
AFTER=$(stat -f%z output.bin)                  # Still 104857600 bytes
echo "Before: $BEFORE, After: $AFTER"
```

## Building

```bash
# Release build (optimized)
cargo build --release

# The binary will be at ./target/release/randd
./target/release/randd --help
```

### Cross-Compilation

To build for different architectures, you can use Rust's cross-compilation capabilities or the [cross](https://github.com/cross-rs/cross) tool:

```bash
# Install cross
cargo install cross --git https://github.com/cross-rs/cross

# Build for ARM64 Linux
cross build --release --target aarch64-unknown-linux-gnu

# Build for ARMv7 Linux
cross build --release --target armv7-unknown-linux-gnueabihf
```

See the `Cross.toml` file in the repository root for cross-compilation configuration.

## CI/CD

This project uses GitHub Actions for automated building and testing across multiple platforms.

### Supported Platforms

The CI pipeline builds binaries for the following platforms:

| Platform | Architecture | Artifact Name |
|----------|--------------|---------------|
| Linux | x86_64 (AMD64) | `randd-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 (ARM64) | `randd-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | armv7 (ARM 32-bit) | `randd-armv7-unknown-linux-gnueabihf.tar.gz` |
| Linux | x86_64-musl | `randd-x86_64-unknown-linux-musl.tar.gz` |
| Linux | aarch64-musl | `randd-aarch64-unknown-linux-musl.tar.gz` |
| macOS | aarch64 (Apple Silicon) | `randd-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 (AMD64) | `randd-x86_64-pc-windows-msvc.zip` |

### Workflow Triggers

The CI/CD pipeline runs on:
- **Push** to `master` or `main` branches
- **Pull Requests** to `master` or `main` branches  
- **Releases** (automatically creates GitHub release artifacts)

### Build Process

For each platform, the pipeline:
1. Installs the appropriate Rust toolchain
2. Uses `cross` for cross-platform builds (when needed)
3. Builds the optimized release binary
4. Strips the binary to reduce size
5. Creates compressed archives (`.tar.gz` for Linux/macOS, `.zip` for Windows)
6. Generates SHA256 checksums for verification
7. Uploads artifacts as GitHub Actions artifacts
8. On release, uploads to the GitHub Release

### Artifacts

All builds include:
- The compiled binary for the target platform
- A compressed archive (tarball or zip)
- SHA256 checksum for verification

### Local Testing with Cross

Before pushing changes, you can test cross-compilation locally using `cross`:

```bash
# Install cross
cargo install cross --git https://github.com/cross-rs/cross

# Test build for a specific target
cross build --release --target aarch64-unknown-linux-gnu

# Test with cross
cross test --target aarch64-unknown-linux-gnu
```

## Downloading Pre-built Binaries

Pre-built binaries are available on the [Releases](../../releases) page. Choose the appropriate package for your platform:

### Example: Installing on Linux

```bash
# Download
wget https://github.com/OWNER/randd/releases/latest/download/randd-x86_64-unknown-linux-gnu.tar.gz

# Verify checksums
wget https://github.com/OWNER/randd/releases/latest/download/randd-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c randd-x86_64-unknown-linux-gnu.tar.gz.sha256

# Extract
tar -xzf randd-x86_64-unknown-linux-gnu.tar.gz

# Install
sudo mv randd /usr/local/bin/
randd --version
```

## Command-Line Options

```
randd 0.1.0

Usage: randd [OPTIONS] --bs <BS>

Options:
  -i, --if <INPUT>          Input file [default: stdin]
  -o, --of <OUTPUT>         Output file [default: stdout] (must exist)
  -b, --bs <BS>             Block size (format: SIZE or MIN-MAX)
      --count <COUNT>       Number of blocks to copy
      --skip <SKIP>         Skip N input blocks
      --seek <SEEK>         Seek N output blocks before starting
      --speed <SPEED>       Limit copy speed (supports size suffixes)
  -s, --conv <CONV>         Conversion options (comma-separated)
      --status <STATUS>     Status mode: progress, bitarray, noxfer, none
  -h, --help               Print help
  -V, --version            Print version
```

## Use Cases

### 1. Stress Testing

Write random data to random positions to test disk endurance and error handling:

```bash
randd --if /dev/urandom --of /dev/sdx --bs 16M --count 1000 --status bitarray
```

### 2. Disk Corruption Simulation

Randomly overwrite parts of a disk to simulate corruption:

```bash
randd --if /dev/random --of /dev/sdb --bs 8M-32M --status bitarray
```

### 3. Random Access Pattern Testing

Test drive performance with random access patterns:

```bash
randd --if /dev/zero --of /dev/sdc --bs 4K --count 100000 --speed 500M
```

### 4. File Destruction

Securely destroy file contents by random overwriting:

```bash
randd --if /dev/urandom --of sensitive.bin --bs 1M --count 100 --status bitarray
```

## Warning

**⚠️ EXTREME CAUTION REQUIRED ⚠️**

Writing to block devices with `randd` will **completely destroy data** on those devices. The tool writes to random positions, making recovery extremely difficult.

- **NEVER** use on drives containing important data
- **ONLY** use on drives you intend to wipe or destroy
- **BACK UP** all important data before using
- **VERIFY** you're using the correct device path

## License

This project is provided as-is for educational and testing purposes.