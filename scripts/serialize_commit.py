#!/usr/bin/python3

import sys
import os
from pygit2 import Repository
import toml


# ------------------------
# Script Environment

module_file = ''
module_path = os.path.abspath(__file__)
main_dir = ''
work_dir = ''
commit_file = 'config_forest_commit.toml'


slash_pos = module_path.rfind('/', 0)

if slash_pos != -1:
    work_dir = module_path[0: slash_pos + 1]
    module_file = module_path[slash_pos + 1: len(module_path)]
else:
    module_file = module_path

if work_dir != '':
    slash_pos = work_dir.rfind('/', 0, -1)
    if slash_pos != -1:
        main_dir = work_dir[0: slash_pos + 1]
    else:
        main_dir = work_dir


# ------------------------
# Script Parameter

module_debug = False
module_quiet = False
module_rs = 0

for arg in sys.argv:
    if arg[0: 1] == '--':
        arg = arg[2: len(arg)]
        if arg == 'debug':
            module_debug = True
        elif arg == 'quiet':
            module_quiet = True

    elif arg[0] == '-':
        arg = arg[1: len(arg)]
        for idx in range(0, len(arg)):
            if arg[idx] == 'd':
                module_debug = True
            elif arg[idx] == 'q':
                module_quiet = True


# ------------------------
# Get the Commit ID of HEAD

commit_reference = 'HEAD'
commit_hash = ''
commit_hash_short = ''

try:
    # ------------------------
    # Execute the Git Command

    repo = Repository(main_dir + '.git')

    commit_hash = str(repo.revparse_single(commit_reference).id)
    commit_hash_short = commit_hash[0: 8]
except Exception as e:
    if not module_quiet:
        print("script '{}' - Git: Git Command failed!".format(module_file),
              file=sys.stderr)
        print("script '{}' - Git Exception Message: {}".format(
            module_file, str(e)), file=sys.stderr)

    module_rs = 1


if commit_hash != '':
    # ------------------------
    # Serialize Git Commit

    forest_commit = {'current_commit': {
        'hash': commit_hash, 'short': commit_hash_short}}

    if module_debug:
        print("script '{}' - Forest Commit:\n{}".format(module_file, str(forest_commit)))

    try:
        with open(main_dir + commit_file, 'w') as f:
            forest_commit_str = toml.dump(forest_commit, f)

        if module_debug:
            print(
                "script '{}' - Forest Commit:\n{}".format(module_file, forest_commit_str))

        if not module_quiet:
            print("script '{}' - Commit File '{}': File saved.".format(
                module_file, commit_file))

    except Exception as e:
        if not module_quiet:
            print("script '{}' - Commit File '{}': Save File failed!".format(
                module_file, commit_file), file=sys.stderr)
            print("script '{}' - Commit File Exception Message: {}".format(
                module_file, str(e)), file=sys.stderr)

        module_rs = 1


if module_debug:
    print("script '{}': Script finished with [{}]".format(module_file, module_rs))


sys.exit(module_rs)
