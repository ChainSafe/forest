#!/usr/bin/env python3

from argparse import ArgumentParser

import glob
from pathlib import Path
import tomlkit

parser = ArgumentParser()
parser.add_argument("dependency_to_remove")
args = parser.parse_args()

dependency_to_remove: str = args.dependency_to_remove

for file in map(Path, glob.glob("**/*.toml", recursive=True)):
    print(f"{file=}")
    toml = tomlkit.loads(file.read_text())
    done_edit = False
    for table in ["dependencies", "dev-dependencies"]:
        if table in toml:
            if dependency_to_remove in toml[table]:
                print(f"\t{table}contains {dependency_to_remove}")
                toml[table].remove(dependency_to_remove)
                done_edit = True
                if "forest" not in toml[table]:
                    print(f"\tinsert forest in {table}")
                    toml[table].add("forest", {"workspace": True})
    if done_edit:
        file.write_text(tomlkit.dumps(toml))
