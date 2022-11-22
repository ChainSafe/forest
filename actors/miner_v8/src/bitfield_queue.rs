// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::TryInto;

use cid::Cid;
use fil_actors_runtime_v8::{ActorDowncast, Array};
use fvm_ipld_amt::Error as AmtError;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::{ChainEpoch, QuantSpec};
use itertools::Itertools;

/// Wrapper for working with an AMT[ChainEpoch]*Bitfield functioning as a queue, bucketed by epoch.
/// Keys in the queue are quantized (upwards), modulo some offset, to reduce the cardinality of keys.
pub struct BitFieldQueue<'db, BS> {
    pub amt: Array<'db, BitField, BS>,
    quant: QuantSpec,
}

impl<'db, BS: Blockstore> BitFieldQueue<'db, BS> {
    pub fn new(store: &'db BS, root: &Cid, quant: QuantSpec) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Array::load(root, store)?,
            quant,
        })
    }

    /// Adds values to the queue entry for an epoch.
    pub fn add_to_queue(&mut self, raw_epoch: ChainEpoch, values: &BitField) -> anyhow::Result<()> {
        if values.is_empty() {
            // nothing to do.
            return Ok(());
        }

        let epoch: u64 = self.quant.quantize_up(raw_epoch).try_into()?;

        let bitfield = self
            .amt
            .get(epoch)
            .map_err(|e| e.downcast_wrap(format!("failed to lookup queue epoch {}", epoch)))?
            .cloned()
            .unwrap_or_default();

        self.amt
            .set(epoch, &bitfield | values)
            .map_err(|e| e.downcast_wrap(format!("failed to set queue epoch {}", epoch)))?;

        Ok(())
    }

    pub fn add_to_queue_values(
        &mut self,
        epoch: ChainEpoch,
        values: impl IntoIterator<Item = u64>,
    ) -> anyhow::Result<()> {
        self.add_to_queue(epoch, &BitField::try_from_bits(values)?)
    }

    /// Cut cuts the elements from the bits in the given bitfield out of the queue,
    /// shifting other bits down and removing any newly empty entries.
    ///
    /// See the docs on `BitField::cut` to better understand what it does.
    pub fn cut(&mut self, to_cut: &BitField) -> anyhow::Result<()> {
        let mut epochs_to_remove = Vec::<u64>::new();

        self.amt
            .for_each_mut(|epoch, bitfield| {
                let bf = bitfield.cut(to_cut);

                if bf.is_empty() {
                    epochs_to_remove.push(epoch);
                } else {
                    **bitfield = bf;
                }

                Ok(())
            })
            .map_err(|e| e.downcast_wrap("failed to cut from bitfield queue"))?;

        self.amt
            .batch_delete(epochs_to_remove, true)
            .map_err(|e| e.downcast_wrap("failed to remove empty epochs from bitfield queue"))?;

        Ok(())
    }

    pub fn add_many_to_queue_values(
        &mut self,
        values: impl IntoIterator<Item = (ChainEpoch, u64)>,
    ) -> anyhow::Result<()> {
        // Pre-quantize to reduce the number of updates.
        let mut quantized_values: Vec<_> = values
            .into_iter()
            .map(|(raw_epoch, value)| (self.quant.quantize_up(raw_epoch), value))
            .collect();

        // Sort and dedup.
        quantized_values.sort_unstable();
        quantized_values.dedup();

        // Add to queue.
        let mut iter = quantized_values.into_iter().peekable();
        while let Some(&(epoch, _)) = iter.peek() {
            self.add_to_queue_values(
                epoch,
                iter.peeking_take_while(|&(e, _)| e == epoch)
                    .map(|(_, v)| v),
            )?;
        }

        Ok(())
    }

    /// Removes and returns all values with keys less than or equal to until.
    /// Modified return value indicates whether this structure has been changed by the call.
    pub fn pop_until(&mut self, until: ChainEpoch) -> anyhow::Result<(BitField, bool)> {
        let mut popped_values = BitField::new();
        let mut popped_keys = Vec::<u64>::new();

        self.amt.for_each_while(|epoch, bitfield| {
            if epoch as ChainEpoch > until {
                // break
                return Ok(false);
            }

            popped_keys.push(epoch);
            popped_values |= bitfield;
            Ok(true)
        })?;

        if popped_keys.is_empty() {
            // Nothing expired.
            return Ok((BitField::new(), false));
        }

        self.amt.batch_delete(popped_keys, true)?;
        Ok((popped_values, true))
    }
}
