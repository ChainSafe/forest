#!/usr/bin/env python3.11

from pathlib import Path


for folder in filter(Path.is_dir, Path("src").iterdir()):
    for subfolder in filter(Path.is_dir, folder.iterdir()):
        if subfolder.name == "tests":
            modules = list(map(
                lambda path: path.name.removesuffix(".rs"),
                filter(Path.is_file, subfolder.iterdir()),
            ))
            code = [
                "#[cfg(test)]",
                "mod tests {",
                *(f"mod {module};" for module in modules),
                "}",
            ]
            with folder.joinpath("mod.rs").open("a") as f:
                f.writelines(code)
