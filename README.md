# rmx - Fast Parallel Directory Deletion for Windows

A high-performance command-line tool for deleting large directories like `node_modules` and `target` on Windows.

## Performance

Benchmark on 5,301 items (5,000 files, 301 directories):

| Method | Time | vs rmx |
|--------|------|--------|
| **rmx** | 514ms | 1.00x |
| PowerShell Remove-Item | 1,150ms | 2.2x slower |

## Why rmx is Fast

1. **POSIX Delete Semantics** - Uses `FILE_DISPOSITION_POSIX_SEMANTICS` for immediate namespace removal (Windows 10 1607+)
2. **Parallel Deletion** - Multi-threaded workers with dependency-aware scheduling
3. **Direct Windows API** - Bypasses high-level abstractions using `CreateFileW`, `SetFileInformationByHandle`
4. **Long Path Support** - Handles paths >260 characters with `\\?\` prefix
5. **Auto Retry** - Exponential backoff for locked files

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Delete a directory
rmx ./node_modules

# Delete multiple directories
rmx ./target ./node_modules ./dist

# Dry run (preview what would be deleted)
rmx -n ./node_modules

# Verbose mode with statistics
rmx -v --stats ./target

# Ask for confirmation
rmx -c ./important_folder

# Force deletion of dangerous paths
rmx --force ./path
```

## Options

| Option | Description |
|--------|-------------|
| `-t, --threads <N>` | Number of worker threads (default: CPU count) |
| `-n, --dry-run` | Scan but don't delete |
| `-v, --verbose` | Show progress and errors |
| `-c, --confirm` | Ask for confirmation |
| `--stats` | Show detailed statistics |
| `--force` | Force deletion of dangerous paths |

## Safety Features

- **System directories protected** - Cannot delete `C:\Windows`, `C:\Program Files`, etc.
- **Home directory protected** - Cannot delete user's home directory
- **Current directory check** - Warns when deleting CWD or its parents
- **Confirmation option** - Use `-c` for interactive confirmation

## Technical Details

### Windows API Usage

- `CreateFileW` with `FILE_SHARE_DELETE` for non-blocking access
- `SetFileInformationByHandle` with `FILE_DISPOSITION_INFORMATION_EX`
- `FILE_DISPOSITION_POSIX_SEMANTICS` for immediate removal
- `FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE` for read-only files
- `FindFirstFileExW` / `FindNextFileW` for fast enumeration

### File Lock Handling

When a file is locked by another process:
1. Retry up to 10 times with exponential backoff (10ms → 100ms)
2. If still locked, record failure and continue with other files
3. Report all failures at the end

## Requirements

- Windows 10 version 1607 or later
- NTFS filesystem

## License

MIT OR Apache-2.0
