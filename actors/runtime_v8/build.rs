static NETWORKS: &[(&str, &[&str])] = &[
    ("mainnet", &["sector-32g", "sector-64g"]),
    (
        "caterpillarnet",
        &[
            "sector-512m",
            "sector-32g",
            "sector-64g",
            "small-deals",
            "short-precommit",
            "min-power-2k",
        ],
    ),
    ("butterflynet", &["sector-512m", "sector-32g", "sector-64g", "min-power-2g"]),
    ("calibrationnet", &["sector-32g", "sector-64g", "min-power-32g"]),
    ("devnet", &["sector-2k", "sector-8m", "small-deals", "short-precommit", "min-power-2k"]),
    (
        "testing",
        &[
            "sector-2k",
            "sector-8m",
            "sector-512m",
            "sector-32g",
            "sector-64g",
            "small-deals",
            "short-precommit",
            "min-power-2k",
            "no-provider-deal-collateral",
        ],
    ),
    (
        "testing-fake-proofs",
        &[
            "sector-2k",
            "sector-8m",
            "sector-512m",
            "sector-32g",
            "sector-64g",
            "small-deals",
            "short-precommit",
            "min-power-2k",
            "no-provider-deal-collateral",
            "fake-proofs",
        ],
    ),
];
const NETWORK_ENV: &str = "BUILD_FIL_NETWORK";

fn main() {
    let network = std::env::var(NETWORK_ENV).ok();
    println!("cargo:rerun-if-env-changed={}", NETWORK_ENV);

    let network = network.as_deref().unwrap_or("mainnet");
    let features = NETWORKS.iter().find(|(k, _)| k == &network).expect("unknown network").1;
    for feature in features {
        println!("cargo:rustc-cfg=feature=\"{}\"", feature);
    }
}
