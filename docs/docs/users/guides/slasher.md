# Forest Slasher Service

A consensus fault detection service for Forest that monitors incoming blocks and detects malicious behaviors like double-mining, time-offset mining, and parent-grinding.

## Configuration

The slasher service is configured through environment variables.

### Environment Variables

- `FOREST_FAULTREPORTER_ENABLECONSENSUSFAULTREPORTER`: Enable/disable the consensus fault reporter (default: false)
- `FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERDATADIR`: Directory for storing slasher data (default: `.forest/slasher`)
- `FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERADDRESS`: Wallet address for submitting fault reports (optional)

### Usage Example

```bash
# Enable the slasher service
export FOREST_FAULTREPORTER_ENABLECONSENSUSFAULTREPORTER=true

# Set the data directory (optional, defaults to .forest/slasher)
export FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERDATADIR="/path/to/slasher/directory"

# Set the reporter address (optional)
export FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERADDRESS="f1abc123..."
```

### Fault Detection

The service detects three types of consensus faults:

1. **Double-fork mining**: Same miner produces multiple blocks at the same epoch
2. **Time-offset mining**: Same miner produces multiple blocks with the same parents
3. **Parent-grinding**: Miner ignores their own block and mines on others
