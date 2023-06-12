#!/usr/bin/env python3.11

from pathlib import Path
import shutil


def folder_is_empty(path: Path) -> bool:
    return sum(1 for _ in path.iterdir()) == 0


cargo_tomls = dict[str, dict]()
all_modules = list[str]()

for key, value in dict(
    auth="./utils/auth",
    beacon="./blockchain/beacon",
    blocks="./blockchain/blocks",
    chain="./blockchain/chain",
    chain_sync="./blockchain/chain_sync",
    cli_shared="./forest/shared",
    db="./node/db",
    deleg_cns="./blockchain/consensus/deleg_cns",
    fil_cns="./blockchain/consensus/fil_cns",
    genesis="./utils/genesis",
    interpreter="./vm/interpreter",
    ipld="./ipld",
    json="./utils/json",
    key_management="./key_management",
    libp2p="./node/forest_libp2p",
    libp2p_bitswap="./node/forest_libp2p/bitswap",
    message="./vm/message",
    message_pool="./blockchain/message_pool",
    metrics="./utils/metrics",
    networks="./networks",
    rpc="./node/rpc",
    rpc_api="./node/rpc-api",
    rpc_client="./node/rpc-client",
    shim="./utils/forest_shim",
    state_manager="./blockchain/state_manager",
    state_migration="./vm/state_migration",
    statediff="./utils/statediff",
    test_utils="./utils/test_utils",
    utils="./utils/forest_utils",
).items():
    all_modules.append(key)

    crate_root = Path(value)  # ./utils/auth
    crate_src = crate_root.joinpath("src")  # ./utils/auth/src
    crate_tests = crate_root.joinpath("tests")  # ./utils/auth/tests
    crate_cargo_toml = crate_root.joinpath("Cargo.toml")  # ./utils/auth/Cargo.toml

    destination_folder = Path("src").joinpath(key)  # ./src/auth

    destination_folder.mkdir(parents=True)

    # ./utils/auth/src/lib.rs -> ./src/auth/mod.rs
    shutil.move(
        crate_src.joinpath("lib.rs"),
        destination_folder.joinpath("mod.rs"),
    )

    for child in crate_src.iterdir():
        shutil.move(child, destination_folder)
    crate_src.rmdir()

    if crate_tests.is_dir():
        shutil.move(crate_tests, destination_folder.joinpath("tests"))

    crate_cargo_toml = crate_root.joinpath("Cargo.toml")
    crate_cargo_toml.unlink()

    if folder_is_empty(crate_root):
        crate_root.rmdir()
    else:
        print(f"dirty: {key} at {value}")

Path("src").joinpath("lib.rs").write_text(
    "\n".join(f"mod {module};" for module in all_modules)
)
