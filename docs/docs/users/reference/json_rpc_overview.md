---
title: JSON-RPC Overview
sidebar_position: 3
---

# JSON-RPC Overview

:::warning

This API is still a WIP, with more methods being added continuously.

:::

:::note

Need a specific method? Let us know on
[Github](https://github.com/ChainSafe/forest/issues) or Filecoin Slack
(`#fil-forest-help`) 🙏

:::

## Overview

The RPC interface is the primary mechanism for interacting with Forest.

Before June 2025, the Lotus
[V0](https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v0-deprecated.md)
and
[V1](https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v1-stable.md)
APIs served as the reference for Forest's implementation.

Since then, the [Common Node API](https://github.com/filecoin-project/FIPs/blob/master/FRCs/frc-0104.md)
 has been the standard for RPC methods across Filecoin clients, replacing the Lotus-specific API as
 the primary reference for V1 methods.

:::info

An FIP to establish a canonical RPC API specification for general use [has been proposed](https://github.com/filecoin-project/FIPs/pull/1027).

:::

## Connecting To A Node

By default, Forest exposes the RPC API on `localhost:2345`. See [CLI docs](./cli.md) for configuration options.

### Authentication

Access control is implemented for certain methods. Levels of access include:

- Read
- Write
- Admin

Authentication is performed via [JWT Tokens](../knowledge_base/jwt_handling.md). When starting Forest use `--save-token 
<FILE>` to store an `Admin` token,
otherwise the token will be printed in the logs during startup. With this token you can call the methods `AuthNew`
to generate additional tokens as needed.
