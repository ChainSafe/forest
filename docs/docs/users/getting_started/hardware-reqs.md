---
title: Hardware Requirements
sidebar_position: 1
---

Forest is designed to be lightweight enough to run on consumer hardware. Below are recommendations for the minium and recommended hardware requirements for running a Forest node, depending on the use case. All requirements assume running the latest version of Forest on mainnet. The requirements on test networks are significantly lower (yes, solar-powered Raspberry Pi 5 is totally fine, check the bottom of the page).

As a rule of thumb, an RPC node would require `200 GiB + 5 GiB per day of retention` of disk space. For example, if you want to retain 30 days of historical state, you would need `200 GiB + (5 GiB * 30) = 350 GiB` of disk space.

Also, if you disable GC, you can cut the disk space requirements in half, but you will need to manage the disk space manually (it's growing by ~2 GiB per day).

## RPC Node (low traffic, minimal retention)

Memory and CPU depend highly on the expected load and enabled RPC methods. Disk space depends on the required historical state to retain.

|            | Minimum | Recommended | Notes                                     |
| ---------- | ------- | ----------- | ----------------------------------------- |
| CPU        | 4-core  | 6-core      |                                           |
| Memory     | 8 GiB   | 16 GiB      | Network upgrades can require more memory. |
| Disk Space | 256 GiB | 256 GiB     | SSD (high IOPS/NVMe recommended)          |

## RPC Node (2 months retention)

This setup should be sufficient for a self-hosted RPC node that retains 2 months of historical state.

Low-traffic is considered under 100 requests per minute. Ultimately, the CPU and memory requirements depend on the combination of request types and their frequency.

|            | Low traffic | High traffic | Notes                                     |
| ---------- | ----------- | ------------ | ----------------------------------------- |
| CPU        | 6-core      | 8-core       |                                           |
| Memory     | 16 GiB      | 32 GiB       | Network upgrades can require more memory. |
| Disk Space | 500 GiB     | 500 GiB      | SSD (high IOPS/NVMe recommended)          |

## Bootstrap Node (stateless)

|            | Minimum | Recommended | Notes                                              |
| ---------- | ------- | ----------- | -------------------------------------------------- |
| CPU        | 2-core  | 4-core      |                                                    |
| Memory     | 2 GiB   | 4 GiB       | Stateless nodes don't need migrations so no spikes |
| Disk Space | 3 GiB   | 3 GiB       | State is not stored, snapshots are not required    |

To properly serve the network, a bootstrap node should ensure at least 6 Mbps upload and 2 Mbps download bandwidth.

## Community: Portable Solar-Powered Forest Node

More of a curiosity - [direct Twitter link](https://platform.twitter.com/embed/Tweet.html?id=1937542522387026383) - but it's worth noting that Forest **can** run on a limited hardware setup. Probably not with an SD card.

<iframe
  src="https://platform.twitter.com/embed/Tweet.html?id=1937542522387026383"
  width="550"
  height="600"
  style={{border: 'none', maxWidth: '100%'}}
  allowFullScreen
></iframe>
