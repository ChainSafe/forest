---
title: Proxy overview
sidebar_position: 1
---

# Proxy overview

## Introduction

Directly exposing a Filecoin node's RPC interface is unsafe for production use. Some reasons include:

Certain methods are very expensive and can potentially be abused. The current JWT protection mechanism has only mutability checks.

- Certain methods should not be exposed to the public at all, such as `Filecoin.ChainExport`.
- There is no rate-limiting mechanism in place. This can lead to a denial-of-service attack and crashing the node.
- On top of node-level caching, proxies can implement additional caching layers to reduce load on the node.

Therefore, running a Filecoin node behind a proxy is recommended.
