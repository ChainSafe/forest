#!/usr/bin/env python3.11

from glob import glob
from pathlib import Path
from subprocess import run

OLD_CRATE_NAMES = [
    "forest_auth",
    "forest_beacon",
    "forest_blocks",
    "forest_chain",
    "forest_chain_sync",
    "forest_cli_shared",
    "forest_db",
    "forest_deleg_cns",
    "forest_fil_cns",
    "forest_genesis",
    "forest_interpreter",
    "forest_ipld",
    "forest_json",
    "forest_key_management",
    "forest_libp2p",
    "forest_libp2p_bitswap",
    "forest_message",
    "forest_message_pool",
    "forest_metrics",
    "forest_networks",
    "forest_rpc",
    "forest_rpc_api",
    "forest_rpc_client",
    "forest_shim",
    "forest_state_manager",
    "forest_state_migration",
    "forest_statediff",
    "forest_test_utils",
    "forest_utils",
]

# If any new module uses `crate::`, it was meant to be `crate::new_mode_name`
for old_crate_name in OLD_CRATE_NAMES:
    new_module_name = old_crate_name.removeprefix("forest_")
    new_module_path = Path("src").joinpath(new_module_name)
    run(
        ["sd", "crate", f"crate::{new_module_name}", *(new_module_path.glob("**/*.rs"))]
    )

    # we accidentally barf these - undo
    run(
        [
            "sd",
            "--string-mode",
            "pub(crate",
            f"pub(in crate",
            *(new_module_path.glob("**/*.rs")),
        ]
    )

# If any crate uses `forest_shim`, it should now be `crate::shim`
for old_crate_name in OLD_CRATE_NAMES:
    new_module_name = old_crate_name.removeprefix("forest_")
    new_module_path = Path("src").joinpath(new_module_name)
    run(
        [
            "sd",
            old_crate_name,
            f"crate::{new_module_name}",
            *Path("src").glob("**/*.rs"),
        ]
    )
