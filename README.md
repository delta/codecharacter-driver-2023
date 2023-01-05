## Requirements

1. Install [Rust](https://www.rust-lang.org/tools/install)

2. Switch to Rust Nightly
   ```
   rustup install nightly && rustup default nightly
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

3. Run
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
