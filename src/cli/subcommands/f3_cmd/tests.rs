// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[test]
fn test_manifest_template() {
    // lotus f3 manifest --output json
    let lotus_json = serde_json::json!({
      "Pause": false,
      "ProtocolVersion": 4,
      "InitialInstance": 0,
      "BootstrapEpoch": 2081674,
      "NetworkName": "calibrationnet",
      "ExplicitPower": null,
      "IgnoreECPower": false,
      "InitialPowerTable": {
        "/": "bafy2bzaceab236vmmb3n4q4tkvua2n4dphcbzzxerxuey3mot4g3cov5j3r2c"
      },
      "CommitteeLookback": 10,
      "CatchUpAlignment": 15000000000_u64,
      "Gpbft": {
        "Delta": 6000000000_u64,
        "DeltaBackOffExponent": 2_f64,
        "MaxLookaheadRounds": 5,
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
        "HeadLookback": 0,
        "Finalize": true
      },
      "CertificateExchange": {
        "ClientRequestTimeout": 10000000000_u64,
        "ServerRequestTimeout": 60000000000_u64,
        "MinimumPollInterval": 30000000000_u64,
        "MaximumPollInterval": 120000000000_u64
      }
    });
    let manifest: F3Manifest = serde_json::from_value(lotus_json).unwrap();
    let template = ManifestTemplate::new(manifest);
    println!("{}", template.render_once().unwrap());
}

#[test]
fn test_progress_template() {
    let lotus_json = serde_json::json!({
      "ID": 1000,
      "Round": 0,
      "Phase": 0
    });
    let progress: F3Instant = serde_json::from_value(lotus_json).unwrap();
    let template = ProgressTemplate::new(progress);
    println!("{}", template.render_once().unwrap());
}

#[test]
fn test_finality_certificate_template() {
    // lotus f3 c get --output json 6204
    let lotus_json = serde_json::json!({
        "GPBFTInstance": 6204,
        "ECChain": [
          {
            "Epoch": 2088927,
            "Key": "AXGg5AIg1NBjOnFimwUueRXQQzvPbHZO6vXbvqNA1gcomlVrq5MBcaDkAiCaOt71j85kjjq3SZF0NQq03tauEW3iwscIr4Qw0wna+g==",
            "PowerTable": {
              "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
            },
            "Commitments": [
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0
            ]
          },
          {
            "Epoch": 2088928,
            "Key": "AXGg5AIgFn9g3q/ATrgWiWzUYZLrtN/POrkNWFPmUShj/MDqZ5IBcaDkAiACwpEW4PvUCOIsZRaYhF6W+L1bgGd2TUFLOkATNxvuGgFxoOQCILlKPpFgMxXYFcq2HslyxzBN9ZZ6iPrPSBI2uwT4tUAvAXGg5AIgwYDZ217HUZ6nGnm6fnNd5lhep2C02mSYkkjJPf5pOig=",
            "PowerTable": {
              "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
            },
            "Commitments": [
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0,
              0
            ]
          }
        ],
        "SupplementalData": {
          "Commitments": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
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
    let template = FinalityCertificateTemplate::new(cert);
    println!("{}", template.render_once().unwrap());
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
        let (from, to) = F3CertsCommands::parse_range_unvalidated(range).unwrap();
        assert_eq!(from, expected_from);
        assert_eq!(to, expected_to);
    }
}

#[test]
fn test_parse_range_invalid() {
    let invalid_cases = ["10..a", "a..20", "a..", "..b"];
    for range in invalid_cases {
        F3CertsCommands::parse_range_unvalidated(range).unwrap_err();
    }
}
