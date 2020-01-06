#!/usr/bin/env sh

PAT_GPL="^// Copyright.*SPDX-License-Identifier: Apache-2.0\.$"
PAT_OTHER="^// Copyright"

for f in $(find . -type f | egrep '\.(rs)$'); do
	HEADER=$(head -16 $f)
	if [[ $HEADER =~ $PAT_GPL ]]; then
		BODY=$(tail -n +17 $f)
		cat copyright.txt > temp
		echo "$BODY" >> temp
		mv temp $f
	elif [[ $HEADER =~ $PAT_OTHER ]]; then
		echo "Other license was found do nothing"
	else
		echo "$f was missing header" 
		cat copyright.txt $f > temp
		mv temp $f
	fi
done
