// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::QuantSpec;
use bitfield::BitField;
use cid::Cid;
use clock::ChainEpoch;
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use std::collections::HashMap;
use std::error::Error as StdError;

/// Wrapper for working with an AMT[ChainEpoch]*Bitfield functioning as a queue, bucketed by epoch.
/// Keys in the queue are quantized (upwards), modulo some offset, to reduce the cardinality of keys.
pub struct BitFieldQueue<'db, BS> {
    pub amt: Amt<'db, BitField, BS>,
    quant: QuantSpec,
}

impl<'db, BS: BlockStore> BitFieldQueue<'db, BS> {
    pub fn new(store: &'db BS, root: &Cid, quant: QuantSpec) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Amt::load(root, store)?,
            quant,
        })
    }

    /// Adds values to the queue entry for an epoch.
    pub fn add_to_queue(
        &mut self,
        raw_epoch: ChainEpoch,
        values: &BitField,
    ) -> Result<(), Box<dyn StdError>> {
        if values.is_empty() {
            // nothing to do.
            return Ok(());
        }

        let epoch = self.quant.quantize_up(raw_epoch);

        let bitfield = self
            .amt
            .get(epoch as u64)
            .map_err(|e| format!("failed to lookup queue epoch {}: {:?}", epoch, e))?
            .unwrap_or_default();

        self.amt
            .set(epoch as u64, &bitfield | values)
            .map_err(|e| format!("failed to set queue epoch {}: {:?}", epoch, e))?;

        Ok(())
    }

    pub fn add_to_queue_values(
        &mut self,
        epoch: ChainEpoch,
        values: &[u64],
    ) -> Result<(), Box<dyn StdError>> {
        if values.is_empty() {
            Ok(())
        } else {
            self.add_to_queue(epoch, &values.iter().map(|&i| i as usize).collect())
        }
    }

    /// Cut cuts the elements from the bits in the given bitfield out of the queue,
    /// shifting other bits down and removing any newly empty entries.
    ///
    /// See the docs on `BitField::cut` to better understand what it does.
    pub fn cut(&mut self, to_cut: &BitField) -> Result<(), String> {
        let mut epochs_to_remove = Vec::<u64>::new();

        self.amt
            .for_each_mut(|epoch, bitfield| {
                *bitfield = bitfield.cut(to_cut);

                if bitfield.is_empty() {
                    epochs_to_remove.push(epoch);
                }

                Ok(())
            })
            .map_err(|e| format!("failed to cut from bitfield queue: {:?}", e))?;

        self.amt
            .batch_delete(epochs_to_remove)
            .map_err(|e| format!("failed to remove empty epochs from bitfield queue: {:?}", e))?;

        Ok(())
    }

    pub fn add_many_to_queue_values(
        &mut self,
        values: &HashMap<ChainEpoch, Vec<u64>>,
    ) -> Result<(), Box<dyn StdError>> {
        // Update each epoch in-order to be deterministic.
        // Pre-quantize to reduce the number of updates.

        let mut quantized_values = HashMap::<ChainEpoch, Vec<u64>>::with_capacity(values.len());
        let mut updated_epochs = Vec::<ChainEpoch>::with_capacity(values.len());

        for (&raw_epoch, entries) in values {
            let epoch = self.quant.quantize_up(raw_epoch);
            updated_epochs.push(epoch);
            quantized_values.entry(epoch).or_default().extend(entries);
        }

        updated_epochs.sort_unstable();

        for epoch in updated_epochs {
            self.add_to_queue_values(epoch, &quantized_values.remove(&epoch).unwrap_or_default())?;
        }

        Ok(())
    }

    /// Removes and returns all values with keys less than or equal to until.
    /// Modified return value indicates whether this structure has been changed by the call.
    pub fn pop_until(&mut self, until: ChainEpoch) -> Result<(BitField, bool), Box<dyn StdError>> {
        let mut popped_values = Vec::<BitField>::new();
        let mut popped_keys = Vec::<u64>::new();

        self.amt.for_each_while(|epoch, bitfield| {
            if epoch as ChainEpoch > until {
                // break
                return Ok(false);
            }

            popped_keys.push(epoch as u64);
            popped_values.push(bitfield.clone());
            Ok(true)
        })?;

        if popped_keys.is_empty() {
            // Nothing expired.
            return Ok((BitField::new(), false));
        }

        self.amt.batch_delete(popped_keys)?;
        Ok((BitField::union(popped_values.iter()), true))
    }
}
