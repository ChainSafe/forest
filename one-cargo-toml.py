#!/usr/bin/env python3.11

import itertools
from pathlib import Path
import tomllib
from dataclasses import asdict, dataclass, field
from typing import Any, Callable, Iterable, Self, TypeVar


T = TypeVar("T")
U = TypeVar("U")


def group_by(it: Iterable[T], key: Callable[[T], U]) -> list[tuple[U, list[T]]]:
    return map(
        lambda tup: (tup[0], list(tup[1])),  # type: ignore
        itertools.groupby(
            sorted(it, key=key),  # type: ignore
            key,
        ),
    )


@dataclass(frozen=True)
class NonTrivialDep:
    key: str
    table: str
    dep_name: str
    version: str | None = None
    package: str | None = None
    features: list[str] = field(default_factory=list)
    optional: bool | None = None
    default_features: bool | None = None

    @classmethod
    def from_raw(cls, key: str, table: str, dep_name: str, raw: dict) -> Self:
        this = cls(
            key,
            table,
            dep_name,
            raw.pop("version", None),
            raw.pop("package", None),
            raw.pop("features", []),
            raw.pop("optional", None),
            raw.pop("default-features", None),
        )
        if len(raw) != 0:
            raise AssertionError(raw)
        return this


@dataclass(frozen=True)
class Feature:
    key: str
    feat_name: str
    feat_values: list[str]


@dataclass(frozen=True)
class Unhandled:
    key: str
    unhandled: dict


non_trivial_deps = list[NonTrivialDep]()
features = list[Feature]()
unhandled = list[Unhandled]()

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
    crate_root = Path(value)  # ./utils/auth
    crate_cargo_toml = crate_root.joinpath("Cargo.toml")  # ./utils/auth/Cargo.toml
    cargo_toml = tomllib.loads(crate_cargo_toml.read_text())

    cargo_toml.pop("package", None)  # don't care about [package] table

    for table in ["dependencies", "dev-dependencies", "build-dependencies"]:
        for dep_name, dep_value in cargo_toml.pop(table, dict[str, Any]()).items():
            match dep_value:
                case str():
                    non_trivial_deps.append(
                        NonTrivialDep(key, table, dep_name, version=dep_value)
                    )
                case dict():
                    if dep_value == {"workspace": True}:
                        pass
                    else:
                        dep_value.pop("workspace", None)
                        non_trivial_deps.append(
                            NonTrivialDep.from_raw(key, table, dep_name, dep_value)
                        )

    for feat_name, feat_values in cargo_toml.pop(
        "features", dict[str, list[str]]()
    ).items():
        features.append(Feature(key, feat_name, feat_values))

    if not len(cargo_toml) == 0:
        unhandled.append(Unhandled(key, cargo_toml))

lines = []

for table, ntds in group_by(non_trivial_deps, lambda ntd: ntd.table):
    lines.append(f"[{table}]")
    for dep_name, ntds in group_by(ntds, lambda ntd: ntd.dep_name):
        for ntd in ntds:
            d = [
                f"{k}={v}"  # close enough
                for k, v in asdict(ntd).items()
                if v is not None and k not in ["table", "key", "dep_name"]
            ]
            lines.append(f"{dep_name} = {{{','.join(d)}}} # {ntd.key}")
    lines.append("")

lines.extend(["","[features]"])
features.sort(key=lambda x: x.feat_name)
for feature in features:
    lines.append(f"{feature.feat_name} = {feature.feat_values} # {feature.key}")

lines.extend(["", "# unhandled"])
for u in unhandled:
    lines.append(f"{u.unhandled} # {u.key}")

Path("one-cargo-toml.toml").write_text("\n".join(lines))
