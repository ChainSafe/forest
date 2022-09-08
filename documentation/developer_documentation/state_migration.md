It's unclear how we can support migrations without adding a lot of code complexity. This document is meant to shed light on the matter and illuminate a sustainable path forward.
As a start we will consider a migration going from nv15 to nv16.

# Migration path investigation from nv15 to nv16

## Findings

1) Actor IDs definitely changed

For following actors only their CID have changed:
- init
- cron
- account
- power
- miner
- paymentchannel
- multisig
- reward
- verifiedregistry

Those are just simple code migration.

For system and market actors there's both code and state changes. That's why there is dedicated logic for their migration.

The system actor need to update the state tree with its new state that holds now the `ManifestData` CID.

For the market actor more work is involved to upgrade actor state due to support for UTF-8 string label encoding in deal proposals and pending proposals (see [FIP-0027](https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0027.md)).

2) Some gas calculations changed?

I don't think we are concerned by this. Gas metering can change at a given protocol upgrade for one or many actors but the impact is irrelevant as it doesn't modify blockchain data structures. Gas calculations should only impact code and in our case the nv16 ref-fvm is already supporting the new gas changes.

3) drand calculation changed?

Ditto.

4) What else changed?

Nothing else as far I can see.

## Open questions

- pre-migration framework + caching: how much do we need a similar approach in Forest?
  Are there other alternatives? We can definitely skip this part at first.
  For information the old nv12 state migration in forest took around 13-15 secs.

- Seen in Lotus: `UpgradeRefuelHeight`. What's Refuel for?

- Migration logic is in spec-actors (go actors), what the future of this given clients moved to builtin-actors (rust actors) and ref-fvm? In an ideal world we might want a shared migration logic.

- Implement Lite migration?
  > should allow for easy upgrades if actors code needs to change but state does not. Example provided above the function to perform all the migration duties. Check actors_version_checklist.md for the rest of the steps.

- What are non-deferred actors in the context of a migration?

- The `migrationJobResult` struct is using a `states7` actor instead of a `states8` one (in go spec-actors).
  Typo or are there some good reasons?

## Changes rough proposal

To support nv15 to nv16 migration we need to:

- [ ] Make forest sync again on nv15 and be able to support multiple network versions.
- [ ] Understand existing forest migration framework (used in the past for nv12 migration). Can we reuse most of the code as is?
- [ ] Implementation of the nv16 migration logic (replicating same logic as in spec-actors).
- [ ] Implementation of unit tests covering this migration.
- [ ] Implemention of a migration schedule that will select the right migration path.
- [ ] Test migration using the exported calibnet and mainnet snapshots and respectively measure the elapsed time and memory usage.

## Test snapshots

For testing a calibnet migration two snapshots have been exported with Lotus:
- lotus_snapshot_2022-Aug-5_height_1044460.car
- lotus_snapshot_2022-Aug-5_height_1044659.car

They are respectively exported 200 and 1 epochs before the Skyr upgrade (the 200 version could be useful if we decide to implement a pre-migration like in Lotus).

For testing a mainnet migration, one snapshot has been retrieved from Protocol Labs s3 bucket using the [lily-shed](https://github.com/kasteph/lily-shed/) util:
- minimal_finality_stateroots_1955760_2022-07-05_00-00-00.car

This one is 4560 epochs before. If needed we can extract closer snapshots later.

Those snapshots have been uploaded to our Digital Ocean Spaces.

## Additional resources

`what changed` between versions is maintained in the [tpm repo](https://github.com/filecoin-project/tpm/tree/master/Network%20Upgrades), e.g. all the changes in [NV15 -> NV16](https://github.com/filecoin-project/tpm/blob/master/Network%20Upgrades/v16.md)
