// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[test]
fn test_manifest_template() {
    // lotus f3 manifest --output json
    let lotus_json = serde_json::json!({
      "ProtocolVersion": 7,
      "InitialInstance": 0,
      "BootstrapEpoch": 2081674,
      "NetworkName": "calibrationnet",
      "InitialPowerTable": {
        "/": "bafy2bzaceab236vmmb3n4q4tkvua2n4dphcbzzxerxuey3mot4g3cov5j3r2c"
      },
      "CommitteeLookback": 10,
      "CatchUpAlignment": 15000000000_u64,
      "Gpbft": {
        "Delta": 6000000000_u64,
        "DeltaBackOffExponent": 2_f64,
        "QualityDeltaMultiplier": 1_f64,
        "MaxLookaheadRounds": 5,
        "ChainProposedLength": 100,
        "RebroadcastBackoffBase": 6000000000_u64,
        "RebroadcastBackoffExponent": 1.3,
        "RebroadcastBackoffSpread": 0.1,
        "RebroadcastBackoffMax": 60000000000_u64
      },
      "EC": {
        "Period": 30000000000_u64,
        "Finality": 900,
        "DelayMultiplier": 2_f64,
        "BaseDecisionBackoffTable": [
          1.3,
          1.69,
          2.2,
          2.86,
          3.71,
          4.83,
          6.27,
          7.5
        ],
        "HeadLookback": 4,
        "Finalize": true
      },
      "CertificateExchange": {
        "ClientRequestTimeout": 10000000000_u64,
        "ServerRequestTimeout": 60000000000_u64,
        "MinimumPollInterval": 30000000000_u64,
        "MaximumPollInterval": 120000000000_u64
      },
      "PubSub": {
        "CompressionEnabled": false,
        "ChainCompressionEnabled": true,
        "GMessageSubscriptionBufferSize": 128,
        "ValidatedMessageBufferSize": 128
      },
      "ChainExchange": {
        "SubscriptionBufferSize": 32,
        "MaxChainLength": 100,
        "MaxInstanceLookahead": 10,
        "MaxDiscoveredChainsPerInstance": 1000,
        "MaxWantedChainsPerInstance": 1000,
        "RebroadcastInterval": 2000000000_u64,
        "MaxTimestampAge": 8000000000_u64
      },
      "PartialMessageManager": {
        "PendingDiscoveredChainsBufferSize": 100,
        "PendingPartialMessagesBufferSize": 100,
        "PendingChainBroadcastsBufferSize": 100,
        "PendingInstanceRemovalBufferSize": 10,
        "CompletedMessagesBufferSize": 100,
        "MaxBufferedMessagesPerInstance": 25000,
        "MaxCachedValidatedMessagesPerInstance": 25000
      }
    });
    let manifest: `F3`Manifest = serde_json::from_value(lotus_json).unwrap();
    println!("{}", render_manifest_template(&manifest).unwrap());
}

#[test]
fn test_progress_template() {
    let lotus_json = serde_json::json!({
      "ID": 1000,
      "Round": 0,
      "Phase": 0
    });
    let progress: `F3`InstanceProgress = serde_json::from_value(lotus_json).unwrap();
    println!("{}", render_progress_template(&progress).unwrap());
}

#[test]
fn test_finality_certificate_template() {
    // lotus f3 c get --output json 6204
    let lotus_json = serde_json::json!({
      "GPBFTInstance": 6204,
      "ECChain": [
        {
          "Key": [
            {
              "/": "bafy2bzacedknayz2ofrjwbjopek5aqz3z5whmtxk6xn35i2a2ydsrgsvnovzg"
            },
            {
              "/": "bafy2bzacecndvxxvr7hgjdr2w5ezc5bvbk2n5vvocfw6fqwhbcxyimgtbhnpu"
            }
          ],
          "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
          "Epoch": 2088927,
          "PowerTable": {
            "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
          }
        },
        {
          "Key": [
            {
              "/": "bafy2bzacealh6yg6v7ae5oawrfwniyms5o2n7tz2xegvqu7gkeugh7ga5jtze"
            },
            {
              "/": "bafy2bzaceabmfeiw4d55ichcfrsrngeel2lprpk3qbtxmtkbjm5eaezxdpxbu"
            },
            {
              "/": "bafy2bzacec4uupurmazrlwavzk3b5slsy4ye35mwpkepvt2ici3lwbhywvac6"
            },
            {
              "/": "bafy2bzacedaybwo3l3dvdhvhdj43u7ttlxtfqxvhmc2nuzeysjemspp6ne5cq"
            }
          ],
          "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
          "Epoch": 2088928,
          "PowerTable": {
            "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
          }
        }
      ],
      "SupplementalData": {
        "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "PowerTable": {
          "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
        }
      },
      "Signers": [
        0,
        3
      ],
      "Signature": "uYtvw/NWm2jKQj+d99UAG4aiPnpAMSrwAWIusv0XkjsOYYR0fyU4nUM++cAQGO47E2/J8WSDjstLgL+yMVAFC+Tgao4o9ILXIlhqhxObnNZ/Ehanajthif9SaRe1AO69",
      "PowerTableDelta": [
        {
          "ParticipantID": 3782,
          "PowerDelta": "76347338653696",
          "SigningKey": "lXSMTNEVmIdVxJV4clmW35jrlsBEfytNUGTWVih2dFlQ1k/7QQttsUGzpD5JoNaQ"
        }
      ]
    });
    let cert: FinalityCertificate = serde_json::from_value(lotus_json).unwrap();
    println!("{}", render_certificate_template(&cert).unwrap());
}

#[test]
fn test_parse_range_valid() {
    let valid_cases = [
        ("10..20", Some(10), Some(20)),
        ("..20", None, Some(20)),
        ("10..", Some(10), None),
        ("10..10", Some(10), Some(10)),
        ("10..9", Some(10), Some(9)),
    ];
    for (range, expected_from, expected_to) in valid_cases {
        let (from, to) = `F3`CertsCommands::parse_range_unvalidated(range).unwrap();
        assert_eq!(from, expected_from);
        assert_eq!(to, expected_to);
    }
}

#[test]
fn test_parse_range_invalid() {
    let invalid_cases = ["10..a", "a..20", "a..", "..b"];
    for range in invalid_cases {
        `F3`CertsCommands::parse_range_unvalidated(range).unwrap_err();
    }
}
