## Requirements

1. Install [Rust](https://www.rust-lang.org/tools/install)

2. Switch to Rust Nightly

   ```
   rustup install nightly && rustup default nightly
   ```

3. Install pre-commit hooks
   ```
   pip install pre-commit
   pre-commit install
   ```

## Setup

1. Clone the repository
   ```
   git clone <repo-url>
   ```
2. Initialze submodules

   ```
    git submodule update --init
   ```

3. Create config files
   ```
   cp .cargo/config.example.toml .cargo/config.toml
   ```
4. Run
   ```
   cargo run
   ```

## Build

1. Build
   ```
   cargo build
   ```
2. Build for release
   ```
   cargo build --release
   ```
