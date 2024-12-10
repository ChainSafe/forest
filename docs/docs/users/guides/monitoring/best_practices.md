---
title: Best Practices
---

### Node

- **monitor disk space usage**, especially the database size - it can grow quickly, especially around network upgrades
- **monitor the memory usage** - the node can use a lot of memory, especially during sync. Don't let it run too close to the limit
- **monitor the chain sync status** - on average, the node should be able to sync two epochs per minute
- **monitor the number of peers** - the more peers, the better, If a node has no peers, it cannot sync
- **monitor the logs for errors and warnings** - they can indicate potential issues

### Monitoring

- **monitor the monitoring system** - if the monitoring system goes down, you won't know if the node is down
- **set up alerts for critical metrics** - disk space, memory usage, sync status, etc.
- **ensure the persistence of the monitoring system** - if the monitoring system loses data, you won't be able to diagnose issues
- **don't expose monitoring endpoints to the internet** - they are not secured and can leak sensitive information
- **don't set the log levels too high** - this can generate a lot of data and slow down the node
