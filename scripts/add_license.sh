#!/bin/bash

PAT_APA="^// Copyright 2020 ChainSafe Systems // SPDX-License-Identifier: Apache-2.0, MIT$"

valid=true
for file in $(find . -type f -not -path "./target/*" -not -path "./blockchain/beacon/src/drand_api/*" -not -path "./ipld/graphsync/src/message/proto/message.rs" | egrep '\.(rs)$'); do
	header=$(echo $(head -3 $file))
	if ! echo "$header" | grep -q "$PAT_APA"; then
		echo "$file was missing header"
		cat ./scripts/copyright.txt $file > temp
		mv temp $file
		valid=false
	fi
done

# if a header is incorrect, return an OS exit code
if [ "$valid" = false ] ; then
	exit 1
fi
