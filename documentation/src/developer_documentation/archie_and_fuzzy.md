# What and How?

Fuzzy and Archie are two servers hosted in the ChainSafe office. Both belong to
the Forest team and are running a Ubuntu variant.

Archie and Fuzzy are accessible through a CloudFlare tunnel. Add this to your
SSH config (`~/.ssh/config`):

```
Host archie.chainsafe.dev
  ProxyCommand /usr/local/bin/cloudflared access ssh --hostname %h
  User archie

Host fuzzy-forest.chainsafe.dev
  ProxyCommand /usr/local/bin/cloudflared access ssh --hostname %h
  User fuzzy
```

If your SSH key has been added to list of authorized keys, you should be able to
directly ssh into `archie.chainsafe.dev` and `fuzzy-forest.chainsafe.dev`. If
you key hasn't been added, complain loudly in the #forest slack channel.

# Archie

Hardware:

```
Motherboard:  GIGABYTE B550M
CPU:          AMD Ryzen™ 5 5600G
SSD:          4x SAMSUNG 870 QVO 8 TB
RAM:          G.Skill DIMM 32 GB DDR4-3200
```

Archie is currently storing the entire Filecoin graph. In the future, this data
will be served into the Filecoin p2p network.

# Fuzzy

Hardware:

```
Motherboard:  GIGABYTE B550M
CPU:          AMD Ryzen™ 5 5600G
SSD:          1x Seagate FireCuda 530 2 TB
RAM:          G.Skill DIMM 64 GB DDR4-3200
```

Fuzzy is meant to run a variety of long-running tasks.

## Github Action Runner

Fuzzy is using the standard runner with a default configuration. See
https://github.com/actions/runner for details.

The instance can be inspected by running `zellij attach runner` on the server.
The command to start the runner is `cd ~/gc_runner; ./run.sh`.
