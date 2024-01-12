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
- `1` - failed to download archive within 5 retries (any reason)
- `2` - cannot unpack archive: not enough disk space
- `3` - cannot unpack archive: any other reason
- `4` - invalid checksum