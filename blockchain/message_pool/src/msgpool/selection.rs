use super::{allow_negative_chains, create_message_chains, MessagePool, Provider};
use crate::block_prob::block_probabilities;
use crate::msg_chain::MsgChain;
use crate::Error;
use address::Address;
use blocks::Tipset;
use message::SignedMessage;
use num_bigint::BigInt;
use std::cmp::Ordering;
use std::collections::HashMap;

type Pending = HashMap<Address, HashMap<u64, SignedMessage>>;
const MAX_BLOCKS: usize = 15;
impl<T> MessagePool<T>
where
    T: Provider + std::marker::Send + std::marker::Sync + 'static,
{
    pub async fn select_messages(
        &mut self,
        ts: Tipset,
        tq: f64,
    ) -> Result<Vec<SignedMessage>, Error> {
        let cur_ts = self.cur_tipset.read().await.clone();
        self.select_messages_greedy(cur_ts.as_ref(), ts).await
    }

    pub async fn select_messages_optimal(
        &mut self,
        cur_ts: Tipset,
        ts: Tipset,
        tq: f64,
    ) -> Result<Vec<SignedMessage>, Error> {
        let base_fee = self.api.read().await.chain_compute_base_fee(&ts)?;

        // 0. Load messages from the target tipset; if it is the same as the current tipset in
        //    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(&cur_ts, &ts).await?;
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        // 0b. Select all priority messages that fit in the block
        // TODO: Implement guess gas
        let min_gas = 1298450;
        let (mut result, mut gas_limit) = self
            .select_priority_messages(&mut pending, &base_fee, &ts)
            .await?;

        // check if block has been filled
        if gas_limit < min_gas {
            return Ok(result);
        }
        // 1. Create a list of dependent message chains with maximal gas reward per limit consumed
        let mut chains = Vec::new();
        for (actor, mset) in pending.into_iter() {
            chains.append(
                &mut create_message_chains(&self.api, &actor, &mset, &base_fee, &ts).await?,
            );
        }

        // 2. Sort the chains
        chains.sort_by(|a, b| a.compare(&b));
        if !allow_negative_chains(cur_ts.epoch())
            && !chains.is_empty()
            && chains[0].curr().gas_perf < 0.0
        {
            return Ok(result);
        }

        // 3. Parition chains into blocks (without trimming)
        //    we use the full blockGasLimit (as opposed to the residual gas limit from the
        //    priority message selection) as we have to account for what other miners are doing
        let mut next_chain = 0;
        let mut partitions: Vec<Vec<MsgChain>> = Vec::with_capacity(MAX_BLOCKS);
        let mut i = 0;
        while i < MAX_BLOCKS && next_chain < chains.len() {
            let mut gas_limit = types::BLOCK_GAS_LIMIT;
            while next_chain < chains.len() {
                let chain = &chains[next_chain];
                next_chain += 1;
                partitions[i].push(chain.clone());
                gas_limit -= chain.curr().gas_limit;
                if gas_limit < min_gas {
                    break;
                }
            }
            i += 1;
        }

        // 4. Compute effective performance for each chain, based on the partition they fall into
        //    The effective performance is the gasPerf of the chain * block probability
        let block_prob = block_probabilities(tq);
        let mut eff_chains = 0;
        for i in (0..MAX_BLOCKS) {
            for chain in (&mut partitions[i]).iter_mut() {
                chain.set_effective_perf(block_prob[i]);
            }
            eff_chains += partitions[i].len();
        }

        // nullify the effective performance of chains that don't fit in any partition
        for chain in (&mut chains[eff_chains..]).iter_mut() {
            chain.set_null_effective_perf();
        }

        // 5. Resort the chains based on effective performance
        chains.sort_by(|a, b| a.cmp_effective(&b));

        // 6. Merge the head chains to produce the list of messages selected for inclusion
        //    subject to the residual gas limit
        //    When a chain is merged in, all its previous dependent chains *must* also be
        //    merged in or we'll have a broken block
        let mut last = chains.len();
        // for (i, chain) in chains.iter_mut().enumerate() {
        for i in 0..chains.len() {
            // did we run out of performing chains?
            if !allow_negative_chains(cur_ts.epoch()) && chains[i].curr().gas_perf < 0.0 {
                break;
            }

            if chains[i].curr().merged {
                continue;
            }
            // compute the dependencies that must be merged and the gas limit including deps
            let mut chain_gas_limit = chains[i].curr().gas_limit;
            let mut chain_deps = Vec::new();
            let mut cur_chain = Some(chains[i].curr());
            while let Some(curr) = cur_chain {
                if curr.merged {
                    break;
                }
                chain_gas_limit += curr.gas_limit;
                chain_deps.push(curr.clone());
                cur_chain = chains[i].move_backward();
            }

            // does it all fit in a block?
            if chain_gas_limit <= gas_limit {
                // include it with all dependencies
                for i in (chain_deps.len() - 1)..0 {
                    let cur_chain = &mut chain_deps[i];
                    cur_chain.merged = true;
                    result.append(&mut cur_chain.msgs.clone());
                }
                chains[i].curr_mut().merged = true;
                // adjust the effective perf for all subsequent chains
                if let Some(next) = chains[i].next_mut() {
                    if next.eff_perf > 0.0 {
                        next.eff_perf += next.parent_offset;
                        chains[i].move_forward();
                        let mut n = chains[i].next().clone();
                        while let Some(_) = n {
                            chains[i].set_eff_perf();
                            n = chains[i].move_forward();
                        }
                    }
                }
                result.append(&mut chains[i].curr().msgs.clone());
                gas_limit -= chain_gas_limit;

                // resort to account for already merged chains and effective performance adjustments
                // the sort *must* be stable or we end up getting negative gas perfs pushed up.
                let (l, r) = chains.split_at_mut(i + 1);
                r.sort_by(|a, b| a.cmp_effective(&b));
                continue;
            }
            // we can't fit this chain and its dependencies because of block gasLimit -- we are
            // at the edge
            last = i;
            break;
        }
        // 7. We have reached the edge of what can fit wholesale; if we still hae available
        //    gasLimit to pack some more chains, then trim the last chain and push it down.
        //    Trimming invalidaates subsequent dependent chains so that they can't be selected
        //    as their dependency cannot be (fully) included.
        //    We do this in a loop because the blocker might have been inordinately large and
        //    we might have to do it multiple times to satisfy tail packing
        todo!()
    }
    async fn select_messages_greedy(
        &mut self,
        cur_ts: &Tipset,
        ts: Tipset,
    ) -> Result<Vec<SignedMessage>, Error> {
        let base_fee = self.api.read().await.chain_compute_base_fee(&ts)?;

        // 0. Load messages from the target tipset; if it is the same as the current tipset in
        //    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(&cur_ts, &ts).await?;
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        // 0b. Select all priority messages that fit in the block
        // TODO: Implement guess gas
        let min_gas = 1298450;
        let (mut result, mut gas_limit) = self
            .select_priority_messages(&mut pending, &base_fee, &ts)
            .await?;

        // check if block has been filled
        if gas_limit < min_gas {
            return Ok(result);
        }
        // 1. Create a list of dependent message chains with maximal gas reward per limit consumed
        let mut chains = Vec::new();
        for (actor, mset) in pending.into_iter() {
            chains.append(
                &mut create_message_chains(&self.api, &actor, &mset, &base_fee, &ts).await?,
            );
        }

        // 2. Sort the chains
        chains.sort_by(|a, b| a.compare(&b));
        if !allow_negative_chains(cur_ts.epoch())
            && !chains.is_empty()
            && chains[0].curr().gas_perf < 0.0
        {
            return Ok(result);
        }
        // 3. Merge the head chains to produce the list of messages selected for inclusion, subject to
        //    the block gas limit.
        let mut last = chains.len();
        for i in 0..chains.len() {
            if !allow_negative_chains(cur_ts.epoch()) && chains[i].curr().gas_perf < 0.0 {
                break;
            }

            if chains[i].curr().gas_limit <= gas_limit {
                gas_limit -= chains[i].curr().gas_limit;
                result.append(&mut chains[i].curr().msgs.clone());
                continue;
            }
            last = i;
            break;
        }
        // 4. We have reached the edge of what we can fit wholesale; if we still have available gasLimit
        // to pack some more chains, then trim the last chain and push it down.
        // Trimming invalidates subsequent dependent chains so that they can't be selected as their
        // dependency cannot be (fully) included.
        // We do this in a loop because the blocker might have been inordinately large and we might
        // have to do it multiple times to satisfy tail packing.
        'tail_loop: while gas_limit >= min_gas && last < chains.len() {
            chains[last].trim(gas_limit, &base_fee, allow_negative_chains(cur_ts.epoch()));
            if chains[last].curr().valid {
                for i in last..(chains.len() - 1) {
                    if chains[i].compare(&chains[i + 1]) == Ordering::Greater {
                        break;
                    }
                    chains.swap(i, i + 1);
                }
            }

            // select the next (valid and fitting) chain for inclusion
            for i in last..chains.len() {
                if !chains[i].curr().valid {
                    continue;
                }

                // if gas_perf < 0 then we have no more profitable chains
                if !allow_negative_chains(cur_ts.epoch()) && chains[i].curr().gas_perf < 0.0 {
                    break 'tail_loop;
                }

                // does it fit in the block?
                if chains[i].curr().gas_limit <= gas_limit {
                    gas_limit -= chains[i].curr().gas_limit;
                    result.append(&mut chains[i].curr().msgs.clone());
                    continue;
                }
                last += 1;
                continue 'tail_loop;
            }
            break;
        }
        Ok(result)
    }

    async fn get_pending_messages(&self, cur_ts: &Tipset, ts: &Tipset) -> Result<Pending, Error> {
        todo!()
    }
    async fn select_priority_messages(
        &self,
        pending: &mut Pending,
        base_fee: &BigInt,
        ts: &Tipset,
    ) -> Result<(Vec<SignedMessage>, i64), Error> {
        let mut result = Vec::with_capacity(self.config.size_limit_low() as usize);
        let mut gas_limit = types::BLOCK_GAS_LIMIT;
        let min_gas = 1298450;

        // 1. Get priority actor chains
        let mut chains = Vec::new();
        let priority = self.config.priority_addrs();
        for actor in priority.iter() {
            if let Some(mset) = pending.remove(actor) {
                let mut next = create_message_chains(&self.api, actor, &mset, base_fee, ts).await?;
                chains.append(&mut next);
            }
        }
        if chains.is_empty() {
            return Ok((Vec::new(), gas_limit));
        }

        // 2. Sort the chains
        chains.sort_by(|a, b| a.compare(&b));

        if !allow_negative_chains(ts.epoch())
            && !chains.is_empty()
            && chains[0].curr().gas_perf < 0.0
        {
            return Ok((Vec::new(), gas_limit));
        }

        // 3. Merge chains until the block limit, as long as they have non-negative gas performance
        let mut last = chains.len();
        for i in 0..chains.len() {
            if !allow_negative_chains(ts.epoch()) && chains[i].curr().gas_perf < 0.0 {
                break;
            }
            if chains[i].curr().gas_limit <= gas_limit {
                gas_limit -= chains[i].curr().gas_limit;
                result.append(&mut chains[i].curr().msgs.clone());
                continue;
            }
            last = i;
            break;
        }
        'tail_loop: while gas_limit >= min_gas && last < chains.len() {
            //trim, discard negative performing messages
            chains[last].trim(gas_limit, base_fee, allow_negative_chains(ts.epoch()));

            // push down if it hasn't been invalidated
            if chains[last].curr().valid {
                for i in last..chains.len() - 1 {
                    if chains[i].compare(&chains[i + 1]) == Ordering::Greater {
                        break;
                    }
                    chains.swap(i, i + 1);
                }
            }

            // select the next (valid and fitting) chain for inclusion
            for i in last..chains.len() {
                if !chains[i].curr().valid {
                    continue;
                }

                // if gas_perf < 0 then we have no more profitable chains
                if !allow_negative_chains(ts.epoch()) && chains[i].curr().gas_perf < 0.0 {
                    break 'tail_loop;
                }

                // does it fit in the block?
                if chains[i].curr().gas_limit <= gas_limit {
                    gas_limit -= chains[i].curr().gas_limit;
                    result.append(&mut chains[i].curr().msgs.clone());
                    continue;
                }
                last += 1;
                continue 'tail_loop;
            }
            break;
        }
        return Ok((result, gas_limit));
    }
}

#[cfg(tests)]
mod test {
    #[test]
    fn t1() {}
}
