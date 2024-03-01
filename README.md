# Quicksync-rs

How to use:
```
quicksync-rs help
```

Development:
```
cargo run -- help
```

## Exit codes
- `0` - all good
- `1` - failed to download archive within max retries (any reason)
- `2` - cannot unpack archive: not enough disk space
- `3` - cannot unpack archive: any other reason
- `4` - invalid checksum of db
- `5` - cannot verify checksum for some reason
- `6` - cannot create a backup file
- `7` - invalid checksum of archive
- `8` - cannot validate archive checksum