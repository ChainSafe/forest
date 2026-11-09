#!/usr/bin/env python3
"""Checks that a resolved compose config (`docker compose -p <project> ... config --format json` on stdin) stays inside the review project.

Prints `ok` and exits 0, or prints one line per entry that reaches outside the project and exits 1. Exits 2 on unreadable input.
Inside means: every volume and network is named `<project>_*` and none is external or driver-backed by host storage; every bind
mount, build context, secret, and config file is under the worktree, and directory binds are read-only; nothing mounts the docker
socket, joins a host or another container's namespace, adds capabilities, or relaxes security options; built images are tagged
`<project>-*` or `<project>_*`; pulls never replace a local tag; the only fixed container names are the ones passed with --container-name.
"""
import argparse
import json
import os
import sys

NAMESPACES = ("network_mode", "pid", "ipc", "uts", "userns_mode", "cgroup")
SAFE_PULL = (None, "missing", "if_not_present", "never", "build")


def under(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return os.path.commonpath([path, root]) == root


def check_top_level(config, project):
    for kind, key in (("volume", "volumes"), ("network", "networks")):
        for name, spec in (config.get(key) or {}).items():
            spec = spec or {}
            real = spec.get("name") or f"{project}_{name}"
            if not real.startswith(f"{project}_"):
                yield f"{kind} {name}: name {real} is outside project {project}"
            if spec.get("external"):
                yield f"{kind} {name}: external"
            if kind == "volume" and (spec.get("driver_opts") or spec.get("driver") not in (None, "local")):
                yield f"{kind} {name}: driver_opts or a non-local driver can point at host storage"


def check_files(config, worktree):
    for key in ("secrets", "configs"):
        for name, spec in (config.get(key) or {}).items():
            path = (spec or {}).get("file")
            if path and not under(path, worktree):
                yield f"{key} {name}: file {path} is outside {worktree}"


def check_build(name, svc, project, worktree):
    build = svc.get("build")
    if not build:
        return
    image = svc.get("image")
    if image and not image.startswith((f"{project}-", f"{project}_")):
        yield f"service {name}: built image {image} is not tagged {project}-*"
    context = build.get("context", "")
    if not under(context, worktree):
        yield f"service {name}: build context {context} is outside {worktree}"
    if build.get("ssh"):
        yield f"service {name}: build ssh forwards the host agent"
    for ctx_name, ctx in (build.get("additional_contexts") or {}).items():
        if "://" in ctx or ctx.startswith("service:") or not under(ctx, worktree):
            yield f"service {name}: build additional context {ctx_name} {ctx} is outside {worktree}"


def check_mounts(name, svc, worktree):
    for mount in svc.get("volumes") or []:
        if mount.get("type") != "bind":
            continue
        source = mount.get("source", "")
        if source.endswith("docker.sock"):
            yield f"service {name}: mounts the docker socket ({source})"
        elif not under(source, worktree):
            yield f"service {name}: bind source {source} is outside {worktree}"
        elif os.path.isdir(source) and not mount.get("read_only"):
            yield f"service {name}: bind source {source} is a writable directory"


def check_service(name, svc, project, worktree, allowed_names):
    cname = svc.get("container_name")
    if cname and cname not in allowed_names:
        yield f"service {name}: container_name {cname} is not namespaced by the project"
    if svc.get("privileged"):
        yield f"service {name}: privileged"
    for key in NAMESPACES:
        mode = str(svc.get(key) or "")
        if mode == "host" or mode.startswith("container:"):
            yield f"service {name}: {key} {mode}"
    for key in ("devices", "cap_add", "security_opt"):
        if svc.get(key):
            yield f"service {name}: {key}"
    if svc.get("pull_policy") not in SAFE_PULL:
        yield f"service {name}: pull_policy {svc['pull_policy']} can replace a local tag"
    for src in svc.get("volumes_from") or []:
        if str(src).startswith("container:"):
            yield f"service {name}: volumes_from {src}"
    yield from check_build(name, svc, project, worktree)
    yield from check_mounts(name, svc, worktree)


def violations(config, project, worktree, allowed_names):
    yield from check_top_level(config, project)
    yield from check_files(config, worktree)
    for name, svc in (config.get("services") or {}).items():
        yield from check_service(name, svc or {}, project, worktree, allowed_names)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--project", required=True)
    parser.add_argument("--worktree", required=True)
    parser.add_argument("--container-name", action="append", default=[])
    args = parser.parse_args()
    try:
        config = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        print(f"compose-scope: unreadable config: {e}", file=sys.stderr)
        return 2
    if not isinstance(config, dict):
        print("compose-scope: config is not a JSON object", file=sys.stderr)
        return 2
    found = list(violations(config, args.project, args.worktree, set(args.container_name)))
    print("\n".join(found) if found else "ok")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
