#!/bin/bash
#
# Checks if the source code contains required license and adds it if necessary.
# Returns 1 if there was a missing license, 0 otherwise.

PAT_APA="^// Copyright 2019-2023 ChainSafe Systems// SPDX-License-Identifier: Apache-2.0, MIT$"

ret=0
for file in $(git grep --cached -Il '' -- '*.rs' ':!*src/utils/encoding/fallback_de_ipld_dagcbor.rs'); do
  header=$(head -2 "$file" | tr -d '\n')
	if ! echo "$header" | grep -q "$PAT_APA"; then
		echo "$file was missing header"
		cat ./scripts/copyright.txt "$file" > temp
		mv temp "$file"
		ret=1
	fi
done

exit $ret
