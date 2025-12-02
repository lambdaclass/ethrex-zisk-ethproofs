# Ethrex ZisK EthProofs Client

> [!WARNING]
> The current version of this project only supports single-GPU proving using the `cargo-zisk prove` command under-the-hood. Support for distributed proving and server mode will be added in future releases.

## Requirements

- Rust and Cargo (install via [rustup](https://rustup.rs/))
- ZisK toolchain v0.14.0 (see instructions below)
- CUDA Toolkit 12.9 or 13.0 (install via [NVIDIA's guide](https://developer.nvidia.com/cuda-toolkit-archive))

## How to run

### 1. Install ZisK GPU toolchain

1. Ensure you have the necessary dependencies and hardware (see [ZisK's installation guide](https://0xpolygonhermez.github.io/zisk/getting_started/installation.html#installation-guide)).
2. Install the ZisK toolchain for GPU proving (if you have any troubles during installation, please refer to the [ZisK installation guide](https://0xpolygonhermez.github.io/zisk/getting_started/installation.html#installation-guide)):

    ```bash
    # Clone the ZisK repository:
    git clone https://github.com/0xPolygonHermez/zisk
    cd zisk
    git checkout v0.14.0

    # Build ZisK tools:
    cargo build --release --features gpu

    # Copy the tools to ~/.zisk/bin directory:
    mkdir -p $HOME/.zisk/bin
    LIB_EXT=$([[ "$(uname)" == "Darwin" ]] && echo "dylib" || echo "so")
    cp target/release/cargo-zisk target/release/ziskemu target/release/riscv2zisk target/release/zisk-coordinator target/release/zisk-worker target/release/libzisk_witness.$LIB_EXT target/release/libziskclib.a $HOME/.zisk/bin

    # Copy required files for assembly rom setup (this is only needed on Linux x86_64):
    mkdir -p $HOME/.zisk/zisk/emulator-asm
    cp -r ./emulator-asm/src $HOME/.zisk/zisk/emulator-asm
    cp ./emulator-asm/Makefile $HOME/.zisk/zisk/emulator-asm
    cp -r ./lib-c $HOME/.zisk/zisk

    # Add ~/.zisk/bin to your system PATH:
    PROFILE=$([[ "$(uname)" == "Darwin" ]] && echo ".zshenv" || echo ".bashrc")
    echo >>$HOME/$PROFILE && echo "export PATH=\"\$PATH:$HOME/.zisk/bin\"" >> $HOME/$PROFILE
    source $HOME/$PROFILE
    ```

### 2. Generate the ROM setup from the guest program

1. Download the GPU proving key:

    ```bash
    wget https://storage.googleapis.com/zisk-setup/zisk-provingkey-0.14.0.tar.gz

    tar -xzf zisk-provingkey-0.14.0.tar.gz

    mv provingKey $HOME/.zisk/provingkey
    ```

2. Run the following in the root of the project to generate the ROM setup:

    ```bash
    cargo-zisk rom-setup -e bin/ethproofs-client/elf/ethrex-f1bd0d7-zisk-0.14.0-guest.elf
    ```

### 3. Run the input generator server

In a terminal, run the following from the root of the project:

```bash
cd bin/input-gen-server

BLOCK_MODULUS=<BLOCK_MODULUS> RPC_URL=<RPC_URL> cargo run --release 
```

> [!NOTE]
>
> - Replace `<BLOCK_MODULUS>` with the desired modulus value (default is `1`).
> - Replace `<RPC_URL>` with your Ethereum Mainnet node HTTP JSON-RPC URL.

### 4. Run the EthProofs client

In another terminal, run the following from the root of the project:

```shell
cd bin/ethproofs-client

RUST_LOG=debug \
ETHPROOFS_API_TOKEN=<ETHPROOFS_API_TOKEN> \
ETHPROOFS_API_URL=<ETHPROOFS_API_URL> \
RPC_URL=<RPC_URL> \
cargo run --release --disable-distributed --no-server --keep-input
```

> [!NOTE]
>
> - Replace `<ETHPROOFS_API_TOKEN>` with your EthProofs API token.
> - Replace `<ETHPROOFS_API_URL>` with your EthProofs API (e.g. <https://staging--ethproofs.netlify.app/api/v0>).
> - Replace `<RPC_URL>` with your Ethereum Mainnet node HTTP JSON-RPC URL.
> - Remove the `RUST_LOG=debug` part if you don't want debug logs (they're useful and not too verbose though).
> - Remove the `--keep-input` flag if you don't want to keep the input files after proving (useful for debugging).
>
> As said before, these instructions are for running an EthProofs client relying on the `cargo-zisk prove` command using a single GPU,hence the `--disable-distributed` and `--no-server` flags. Instructions for running this using distributed proving and server mode will be provided in the future.

### Troubleshooting

> [!NOTE]
> This is a placeholder for future troubleshooting tips. Please report any issues you encounter while running the integration tests to help us improve this section.
