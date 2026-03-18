binary := "target/release/cluster"

# Build optimized binary (LTO + size opt + strip — see [profile.release] in Cargo.toml)
build:
    cargo build --release
    @ls -lh {{binary}}

# Run with cargo (debug mode)
run *ARGS:
    cargo run -- {{ARGS}}
