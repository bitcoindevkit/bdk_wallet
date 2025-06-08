set quiet := true
msrv := "1.63"

# list of recipes
default:
  just --list

# format the project code
fmt:
    cargo +nightly fmt

# lint the project, skip examples if using msrv
clippy: fmt
    if rustc --version | fgrep -q "{{msrv}}"; then \
      cargo clippy --workspace --exclude 'example_*' --all-features --tests; \
    else \
      cargo clippy --all-features --tests; \
    fi

# build the project, skip examples if using msrv
build: fmt
    if rustc --version | fgrep -q "{{msrv}}"; then \
      cargo build --workspace --exclude 'example_*' --all-features --tests; \
    else \
      cargo build --all-features --tests; \
    fi

# test the project, skip examples if using msrv
test:
    if rustc --version | fgrep -q "{{msrv}}"; then \
      cargo test --workspace --exclude 'example_*' --all-features --tests; \
    else \
      cargo test --all-features --tests; \
    fi

# clean the project target directory
clean:
    cargo clean

# set the rust version to stable
stable: clean
    rustup override set stable; cargo update

# set the rust version to the msrv and pin dependencies
msrv: clean
    rustup override set {{msrv}}; cargo update; ./ci/pin-msrv.sh