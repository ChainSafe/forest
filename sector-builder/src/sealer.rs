use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;

use crate::error::Result;
use crate::helpers::{retrieve_piece, seal};
use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};
use crate::scheduler::Request;
use crate::store::SectorStore;

const FATAL_NOLOCK: &str = "error acquiring task lock";
const FATAL_RCVTSK: &str = "error receiving seal task";
const FATAL_SNDTSK: &str = "error sending task";
const FATAL_SNDRLT: &str = "error sending result";

pub struct SealerWorker {
    pub id: usize,
    pub thread: Option<thread::JoinHandle<()>>,
}

pub enum SealerInput {
    Seal(StagedSectorMetadata, mpsc::SyncSender<Request>),
    Unseal(
        String,
        Box<SealedSectorMetadata>,
        mpsc::SyncSender<Result<Vec<u8>>>,
    ),
    Shutdown,
}

impl SealerWorker {
    pub fn start<S: SectorStore + 'static>(
        id: usize,
        seal_task_rx: Arc<Mutex<mpsc::Receiver<SealerInput>>>,
        sector_store: Arc<S>,
        prover_id: [u8; 31],
    ) -> SealerWorker {
        let thread = thread::spawn(move || loop {
            // Acquire a lock on the rx end of the channel, get a task,
            // relinquish the lock and return the task. The receiver is mutexed
            // for coordinating reads across multiple worker-threads.
            let task = {
                let rx = seal_task_rx.lock().expects(FATAL_NOLOCK);
                rx.recv().expects(FATAL_RCVTSK)
            };

            // Dispatch to the appropriate task-handler.
            match task {
                SealerInput::Seal(staged_sector, return_channel) => {
                    let sector_id = staged_sector.sector_id;
                    let result = seal(&sector_store.clone(), &prover_id, staged_sector);
                    let task = Request::HandleSealResult(sector_id, Box::new(result));

                    return_channel.send(task).expects(FATAL_SNDTSK);
                }
                SealerInput::Unseal(piece_key, sealed_sector, return_channel) => {
                    let result = retrieve_piece(
                        &sector_store.clone(),
                        &sealed_sector,
                        &prover_id,
                        &piece_key,
                    );

                    return_channel.send(result).expects(FATAL_SNDRLT);
                }
                SealerInput::Shutdown => break,
            }
        });

        SealerWorker {
            id,
            thread: Some(thread),
        }
    }
}
