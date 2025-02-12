Manifest:
  Protocol Version:     {{ ProtocolVersion }}
  Paused:               {{ Pause }}
  Initial Instance:     {{ InitialInstance }}
  Initial Power Table:  {{ initial_power_table_cid }}
  Bootstrap Epoch:      {{ BootstrapEpoch }}
  Network Name:         {{ NetworkName }}
  Ignore EC Power:      {{ IgnoreECPower }}
  Committee Lookback:   {{ CommitteeLookback }}
  Catch Up Alignment:   {{ CatchUpAlignment | format_duration }}

  GPBFT Delta:                        {{ Gpbft.Delta | format_duration }}
  GPBFT Delta BackOff Exponent:       {{ Gpbft.DeltaBackOffExponent }}
  GPBFT Quality Delta Multiplier:     {{ Gpbft.QualityDeltaMultiplier }}
  GPBFT Max Lookahead Rounds:         {{ Gpbft.MaxLookaheadRounds }}
  GPBFT Chain Proposed Length:        {{ Gpbft.ChainProposedLength }}
  GPBFT Rebroadcast Backoff Base:     {{ Gpbft.RebroadcastBackoffBase | format_duration }}
  GPBFT Rebroadcast Backoff Exponent: {{ Gpbft.RebroadcastBackoffExponent }}
  GPBFT Rebroadcast Backoff Spread:   {{ Gpbft.RebroadcastBackoffSpread }}
  GPBFT Rebroadcast Backoff Max:      {{ Gpbft.RebroadcastBackoffMax | format_duration }}

  EC Period:            {{ EC.Period | format_duration }}
  EC Finality:          {{ EC.Finality }}
  EC Delay Multiplier:  {{ EC.DelayMultiplier }}
  EC Head Lookback:     {{ EC.HeadLookback }}
  EC Finalize:          {{ EC.Finalize }}

  Certificate Exchange Client Timeout:    {{ CertificateExchange.ClientRequestTimeout | format_duration }}
  Certificate Exchange Server Timeout:    {{ CertificateExchange.ServerRequestTimeout | format_duration }}
  Certificate Exchange Min Poll Interval: {{ CertificateExchange.MinimumPollInterval | format_duration }}
  Certificate Exchange Max Poll Interval: {{ CertificateExchange.MaximumPollInterval | format_duration }}

  PubSub Compression Enabled:  {{ PubSub.CompressionEnabled }}

  Chain Exchange Subscription Buffer Size:           {{ ChainExchange.SubscriptionBufferSize }}
  Chain Exchange Max Chain Length:                   {{ ChainExchange.MaxChainLength }}
  Chain Exchange Max Instance Lookahead:             {{ ChainExchange.MaxInstanceLookahead }}
  Chain Exchange Max Discovered Chains Per Instance: {{ ChainExchange.MaxDiscoveredChainsPerInstance }}
  Chain Exchange Max Wanted Chains Per Instance:     {{ ChainExchange.MaxWantedChainsPerInstance }}
  Chain Exchange Rebroadcast Interval:               {{ ChainExchange.RebroadcastInterval | format_duration }}
  Chain Exchange Max Timestamp Age:                  {{ ChainExchange.MaxTimestampAge | format_duration }}
