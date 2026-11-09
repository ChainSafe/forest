#!/usr/bin/env python3
import json
import re
import shlex
import sys

GIT_BLOCKED = {"commit", "push"}
GH_BLOCKED = {
    "pr": {"comment", "review", "edit", "merge", "close", "create", "ready", "reopen", "update-branch"},
    "issue": {"comment", "create", "edit", "close", "delete", "reopen", "lock", "unlock", "pin", "unpin", "transfer"},
    "release": {"create", "edit", "delete", "upload"},
    "workflow": {"run"},
}
SEPARATORS = {"&&", "||", ";", "|", "&", "(", ")"}
WRAPPER_VALUE_FLAGS = {
    "env": {"-u", "-C", "-S", "--unset", "--chdir"},
    "sudo": {"-u", "-g", "-h", "-p", "-C", "-U", "-D", "-R", "-T"},
    "time": {"-f", "-o"},
    "xargs": {"-I", "-n", "-P", "-L", "-d", "-E", "-s", "-a"},
    "command": set(),
    "exec": {"-a"},
    "nohup": set(),
    "nice": {"-n", "--adjustment"},
    "ionice": {"-c", "-n", "-p", "--class", "--classdata"},
    "stdbuf": {"-i", "-o", "-e"},
    "setsid": set(),
}
TIMEOUT_VALUE_FLAGS = {"-s", "-k", "--signal", "--kill-after"}
SHELLS = {"bash", "sh", "zsh", "dash", "ksh", "fish"}
KEYWORDS = {"then", "do", "else", "elif", "{", "!", "if", "while", "until"}
FIELD_FLAGS = ("-f", "-F", "--field", "--raw-field", "--input")
ENV_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
LOOSE_SPLIT = re.compile(r"[\s;|&()<>`${}\[\]]+")
UNQUOTE = str.maketrans("", "", "'\"\\")


def newlines_to_semicolons(command):
    out = []
    quote = None
    i = 0
    while i < len(command):
        ch = command[i]
        if ch == "\\" and quote != "'":
            if command[i + 1 : i + 2] == "\n":
                i += 2
                continue
            out.append(ch)
            out.append(command[i + 1 : i + 2])
            i += 2
            continue
        if quote:
            if ch == quote:
                quote = None
        elif ch in "'\"":
            quote = ch
        elif ch == "\n":
            ch = " ; "
        out.append(ch)
        i += 1
    return "".join(out)


def segments(command):
    lexer = shlex.shlex(newlines_to_semicolons(command), posix=True, punctuation_chars=True)
    lexer.whitespace_split = True
    current = []
    for token in lexer:
        if token in SEPARATORS:
            if current:
                yield current
            current = []
        else:
            current.append(token)
    if current:
        yield current


def skip_options(tokens, with_value):
    rest = list(tokens)
    while rest and rest[0].startswith("-"):
        flag = rest.pop(0)
        if flag in with_value and rest:
            rest.pop(0)
    return rest


def git_reason(args):
    rest = skip_options(args, {"-C", "-c", "--git-dir", "--work-tree", "--namespace"})
    if rest and rest[0] in GIT_BLOCKED:
        return f"git {rest[0]} is blocked while forest-review runs"
    return None


def api_method(args):
    method = None
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("-X", "--method") and i + 1 < len(args):
            method = args[i + 1]
            i += 1
        elif a.startswith("--method="):
            method = a.split("=", 1)[1]
        elif a.startswith("-X") and len(a) > 2:
            method = a[2:]
        i += 1
    return method


def has_field(args):
    for a in args:
        for flag in FIELD_FLAGS:
            if a == flag or a.startswith(flag + "=") or (len(flag) == 2 and a.startswith(flag)):
                return True
    return False


def gh_api_reason(args):
    method = api_method(args)
    endpoint = next((a for a in args if not a.startswith("-")), "")
    if endpoint == "graphql" and any("mutation" in a.lower() for a in args):
        return "gh api graphql mutation is blocked while forest-review runs"
    if method is not None and method.upper() != "GET":
        return f"gh api with method {method} is blocked while forest-review runs"
    if method is None and has_field(args):
        return "gh api with request fields (implicit POST) is blocked while forest-review runs"
    return None


def gh_reason(args):
    rest = skip_options(args, {"-R", "--repo"})
    if not rest:
        return None
    group, sub = rest[0], rest[1:]
    if group == "api":
        return gh_api_reason(sub)
    blocked = GH_BLOCKED.get(group)
    if blocked:
        action = next((a for a in sub if not a.startswith("-")), None)
        if action in blocked:
            return f"gh {group} {action} is blocked while forest-review runs"
    return None


def shell_c_string(args):
    for i, a in enumerate(args):
        if not a.startswith("-") or a == "--":
            return None
        if not a.startswith("--") and "c" in a[1:]:
            return args[i + 1] if i + 1 < len(args) else None
    return None


def here_string(args):
    for i, a in enumerate(args[:-1]):
        if a == "<<<":
            return args[i + 1]
    return None


def basename(token):
    return token.rsplit("/", 1)[-1]


def unwrap(tokens):
    while tokens:
        head = basename(tokens[0])
        rest = tokens[1:]
        if head in KEYWORDS or ENV_ASSIGN.match(head):
            tokens = rest
        elif head == "timeout":
            tokens = skip_options(rest, TIMEOUT_VALUE_FLAGS)[1:]
        elif head in WRAPPER_VALUE_FLAGS:
            tokens = skip_options(rest, WRAPPER_VALUE_FLAGS[head])
        else:
            return tokens
    return tokens


def segment_reason(tokens):
    tokens = unwrap(tokens)
    if not tokens:
        return None
    head, rest = basename(tokens[0]), tokens[1:]
    if head in SHELLS:
        inner = shell_c_string(rest) or here_string(rest)
        return command_reason(inner) if inner else None
    if head in ("source", "."):
        inner = here_string(rest)
        return command_reason(inner) if inner else None
    if head == "eval":
        return command_reason(" ".join(rest))
    if head == "git":
        return git_reason(rest)
    if head == "gh":
        return gh_reason(rest)
    return None


def loose_reason(command):
    text = command.replace("\\\n", "").translate(UNQUOTE)
    tokens = [t for t in LOOSE_SPLIT.split(text) if t]
    for i, token in enumerate(tokens):
        name = basename(token)
        # Quote removal can split an option value across separators, so any later blocked word counts.
        blocked = name == "git" and next((a for a in tokens[i + 1 :] if a in GIT_BLOCKED), None)
        if blocked:
            return f"git {blocked} is blocked while forest-review runs"
        reason = gh_reason(tokens[i + 1 :]) if name == "gh" else None
        if reason:
            return reason
    return None


def command_reason(command):
    try:
        parsed = list(segments(command))
    except ValueError:
        return loose_reason(command)
    for tokens in parsed:
        reason = segment_reason(tokens)
        if reason:
            return reason
    return None


def main():
    try:
        payload = json.load(sys.stdin)
        if payload.get("tool_name") != "Bash":
            return 0
        command = payload["tool_input"]["command"]
        if not isinstance(command, str):
            return 0
    except (ValueError, KeyError, TypeError, AttributeError):
        return 0
    reason = command_reason(command)
    if reason:
        print(reason, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
