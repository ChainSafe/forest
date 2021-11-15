#!/bin/bash

cd documentation/book
git init
git add .
git config --global -l
git -c user.name='ci' -c user.email='ci' commit -m 'Deploy documentation'
git push -f -q https://git:${GITHUB_TOKEN}@github.com/ChainSafe/forest HEAD:gh-pages