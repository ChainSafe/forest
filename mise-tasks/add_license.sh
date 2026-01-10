#!/bin/bash
#MISE description="Checks if the source code contains required license and adds it if necessary."

# Returns 1 if there was a missing license, 0 otherwise.

COPYRIGHT="// Copyright 2019-2026 ChainSafe Systems\n// SPDX-License-Identifier: Apache-2.0, MIT"

# Enable lastpipe option to allow while loop to modify variables in the parent shell. See https://www.shellcheck.net/wiki/SC2031
shopt -s lastpipe

missing_license=0
git ls-files '*.rs' ':!src/utils/encoding/fallback_de_ipld_dagcbor.rs' | while read -r file; do
  # Kind of contrived way of matching multiline text, but that's what grep supports. Note escaping dots
  # to match literal dots.
  if ! head -n 2 "$file" | grep -zPo "${COPYRIGHT//./\\.}" > /dev/null; then
    echo "Adding missing license to $file"
    # Adds the license to the top of the file
    {
      echo -e "$COPYRIGHT\n"
      cat "$file"
    } > "$file.tmp" && mv "$file.tmp" "$file"
    missing_license=1
  fi
done
exit $missing_license
