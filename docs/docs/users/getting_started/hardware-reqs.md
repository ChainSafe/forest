---
title: Hardware Requirements
sidebar_position: 1
---

Forest is designed to be lightweight enough to run on consumer hardware. Below are recommendations for the minium and recommended hardware requirements for running a Forest node, depending on the use case. All requirements assume running the latest version of Forest on mainnet. The requirements on test networks are significantly lower (yes, solar-powered Raspberry Pi 5 is totally fine, check the bottom of the page).

## Bootstrap Node (stateless)

|            | Minimum | Recommended | Notes                                              |
| ---------- | ------- | ----------- | -------------------------------------------------- |
| CPU        | 2-core  | 4-core      |                                                    |
| Memory     | 2 GiB   | 4 GiB       | Stateless nodes don't need migrations so no spikes |
| Disk Space | 3 GiB   | 3 GiB       | State is not stored, snapshots are not required    |

To properly serve the network, a bootstrap node should ensure at least 6 Mbps upload and 2 Mbps download bandwidth.

## Regular Node (stateful)

General-purpose node that participates in the network, validates blocks, and maintains the state. Memory and CPU depend highly on the expected load. Disk space depends on the required historical state to retain.

|            | Minimum | Recommended | Notes                                     |
| ---------- | ------- | ----------- | ----------------------------------------- |
| CPU        | 4-core  | 8-core      |                                           |
| Memory     | 8 GiB   | 16 GiB      | State migrations can require more memory. |
| Disk Space | 256 GiB | 256 GiB     | NVMe recommended.                         |

If you disable GC, you can cut the disk space requirements in half, but you will need to manage the disk space manually (it's growing by ~5 GiB per day).

## RPC Node

Memory and CPU depend highly on the expected load and enabled RPC methods. Disk space depends on the required historical state to retain.

|            | Minimum | Recommended | Notes                                     |
| ---------- | ------- | ----------- | ----------------------------------------- |
| CPU        | 4-core  | 8-core      |                                           |
| Memory     | 8 GiB   | 32 GiB      | State migrations can require more memory. |
| Disk Space | 256 GiB | 256 GiB     | NVMe recommended.                         |

As a rule of thumb, an RPC node would require `200 GiB + 5 GiB per day of retention` of disk space. For example, if you want to retain 30 days of historical state, you would need `200 GiB + (5 GiB * 30) = 350 GiB` of disk space.

## RPC Node (low traffic, 2 months retention)

This setup should be sufficient for a self-hosted RPC node that serves a small number of requests (under 100 requests per minute) and retains 2 months of historical state. Note that if the methods called by the clients are more resource-intensive, you might need to tweak the setup.

|            | Recommended | Notes                                                           |
| ---------- | ----------- | --------------------------------------------------------------- |
| CPU        | 6-core      | Possible to run on 4-core, but might struggle at certain times. |
| Memory     | 16 GiB      | Network upgrades can require more memory.                       |
| Disk Space | 500 GiB     | SSD/NVMe recommended.                                           |

## Community: Portable Solar-Powered Forest Node

<iframe
  src="https://platform.twitter.com/embed/Tweet.html?id=1937542522387026383"
  width="550"
  height="600"
  style={{border: 'none', maxWidth: '100%'}}
  allowFullScreen
></iframe>
