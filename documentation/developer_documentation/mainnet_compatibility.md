
# Testing for Mainnet Compatibility

Forest development can be like hitting a moving target and sometimes Forest falls behind the
network. This document should serve as a way to easily identify if Forest can sync all the way
up to the network head using a simple step-by-step process.

## Prerequisites

Some command-line tools and software is required to follow this guide. 

 - A fresh copy of the Forest repository that has been built
 - Lotus installed
 - curl (to download snapshots)
 - sha256sum (optional, used to verify snapshot integrity)


## Grab a snapshot and run Forest

Refer to the mdbook documentation on how to download a snapshot and run forest

Warning: FileCoin snapshots as of this writing are over 75GB. Verify you have enough
space on your system to accomodate these large files.

 - Use `make mdbook` in Forest's root directory
 - Open `http://localhost:3000`
 - Navigate to `2. Basic Usage` in the menu on the right
 - Scroll down to `Forest Import Snapshot Mode`


## Let Forest sync

This step may take a while. We want Forest to get as far along in the syncing process
as it can get. If it syncs up all the way to the network head, CONGRATS! Forest is up to date
and on mainnet. Otherwise, Forest is not on mainnet.

If Forest starts to error and can't get past a block while syncing. Make note of which block it is.
We can use that block to help debug any potential state mismatches.


## Is Forest on the latest network version?

Something easy to check is if Forest is on the latest Filecoin network version. A repository exists
where we can see all of the released network versions [here](https://github.com/filecoin-project/tpm/tree/master/Network%20Upgrades).
Navigate the codebase to see mention of the latest network upgrade. If a snapshot fails to sync at a certain epoch, it's entirely
possible that the snapshot was behind an epoch when a version upgrade started. Grab a new snapshot by referring to the mdbook
documentation.

## Debugging State Mismatches

If an error occurs that mentions "state mismatch". Then follow the following steps:

enable DEBUG logs by adding the following flags to Forest.

`RUST_LOG="error,chain_sync=info,interpreter=debug" forest`

If it's easy to do so, you can compare the failed transactions and see if any should have
succeeded. If you can try to pinpoint and solve the logic from the error message and looking
around the part of code which it was triggered, it will save you some time.

The next step, if there is no low hanging fruit to look into, is to start an export from Forest
that overlaps the failed transaction (to be able to statediff with the expected state).

`forest chain export --skip-old-msgs --tipset @474196 --recent-stateroots 900 ./chain474196.car`

Where you would replace 474196 with any epoch about 100 epochs ahead. Theoretically, this could be not
enough state to verify that epoch, but it's extremely improbable. You could increase --recent-stateroots
if you wanted to be sure there was 900 epochs behind the faulty epoch.
