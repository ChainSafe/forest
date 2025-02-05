---
title: Logs
---

Logs are written to standard output by default. They can be written to rolling log files with the `--log-dir <dir>` flag. The log level can be set with the `RUST_LOG` environment variable. The defaults are generally sufficient for most users but can be adjusted to provide more or less information. Different modules can have different log levels, and the log level can be set to `trace`, `debug`, `info`, `warn`, `error`, or `off`.

```bash
RUST_LOG=info,forest_filecoin=debug forest --chain calibnet
```

Sample output:

```console
2024-08-28T12:49:59.830012Z  INFO forest::daemon::main: Using default calibnet config
2024-08-28T12:49:59.834109Z  INFO forest::daemon: Starting Forest daemon, version 0.19.2+git.74fd562acce
2024-08-28T12:49:59.834123Z DEBUG forest::daemon: Increased file descriptor limit from 1024 to 8192
2024-08-28T12:49:59.834164Z DEBUG forest::libp2p::keypair: Recovered libp2p keypair from /home/rumcajs/.local/share/forest/libp2p/keypair
```

:::tip
Enabling `trace` or `debug` logging can generate gargantuan log files (gigabytes per minute). Make sure to adjust the log level to your needs.
:::

Sending logs to Loki is also possible. Pass `--loki` to the Forest daemon to enable it. The logs are sent to Loki via the HTTP API. The Loki endpoint can be set with the `--loki-endpoint` flag. The default endpoint is `http://localhost:3100`.
