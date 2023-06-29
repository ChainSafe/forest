| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `./target/release/examples/benchmark --mode buffer8k /home/aatif/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-29_height_690463.car` | 3.229 ± 0.248 | 2.946 | 3.660 | 1.32 ± 0.13 |
| `./target/release/examples/benchmark --mode buffer1k /home/aatif/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-29_height_690463.car` | 2.443 ± 0.148 | 2.276 | 2.668 | 1.00 |
| `./target/release/examples/benchmark --mode buffer100 /home/aatif/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-29_height_690463.car` | 2.562 ± 0.305 | 2.149 | 3.043 | 1.05 ± 0.14 |
| `./target/release/examples/benchmark --mode unbuffered /home/aatif/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-29_height_690463.car` | 5.788 ± 0.192 | 5.573 | 6.105 | 2.37 ± 0.16 |
