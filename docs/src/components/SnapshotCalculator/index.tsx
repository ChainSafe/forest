import React, { useState, useMemo, useEffect, type ReactElement } from "react";
import styles from "./SnapshotCalculator.module.css";

const LITE_INTERVAL = 30_000;
const DIFF_INTERVAL = 3_000;
const STATE_DEPTH = 900;
const VALIDATE_LOOKBACK = 2_000;

const ARCHIVE_BASE = "https://forest-archive.chainsafe.dev/archive/forest";

// Well-known genesis timestamps (unix seconds).
const GENESIS_TIMESTAMPS: Record<Network, number> = {
  calibnet: 1667326380, // 2022-11-01T14:13:00Z
  mainnet: 1598306400, // 2020-08-24T22:00:00Z
};

const EPOCH_DURATION_SECONDS = 30;

type Network = "calibnet" | "mainnet";
type Mode = "use" | "validate";

/** Current epoch on the given network (floored). */
function currentEpoch(network: Network): number {
  const genesis = GENESIS_TIMESTAMPS[network];
  return Math.floor((Date.now() / 1000 - genesis) / EPOCH_DURATION_SECONDS);
}

interface SnapshotSpec {
  type: "lite" | "diff";
  /** The height shown in the filename (base height for diffs). */
  height: number;
  range?: number;
  segment: "previous" | "base";
}

interface ResolvedSnapshot extends SnapshotSpec {
  filename: string;
  downloadUrl: string;
}

/** Compute the date string for a given height on a network. */
function heightToDate(network: Network, height: number): string {
  const genesis = GENESIS_TIMESTAMPS[network];
  const ts = genesis + height * EPOCH_DURATION_SECONDS;
  const d = new Date(ts * 1000);
  return d.toISOString().slice(0, 10); // YYYY-MM-DD
}

function makeFilename(spec: SnapshotSpec, network: Network): string {
  if (spec.type === "lite") {
    const date = heightToDate(network, spec.height);
    return `forest_snapshot_${network}_${date}_height_${spec.height}.forest.car.zst`;
  }
  // Diff filenames use the date of the *end* epoch (height + range),
  // matching the Rust code in format_diff_snapshot().
  const date = heightToDate(network, spec.height + (spec.range ?? 0));
  return `forest_diff_${network}_${date}_height_${spec.height}+${spec.range}.forest.car.zst`;
}

function makeUrl(spec: SnapshotSpec, network: Network): string {
  const type = spec.type === "lite" ? "lite" : "diff";
  return `${ARCHIVE_BASE}/${network}/${type}/${makeFilename(spec, network)}`;
}

function computeRequired(epoch: number, mode: Mode): SnapshotSpec[] {
  const specs: SnapshotSpec[] = [];
  const baseEpoch = Math.floor(epoch / LITE_INTERVAL) * LITE_INTERVAL;

  if (mode === "validate") {
    const earliestNeeded = epoch - VALIDATE_LOOKBACK;
    const earliestAvailable = baseEpoch - STATE_DEPTH;
    if (earliestNeeded < earliestAvailable) {
      const prevLiteEpoch = baseEpoch - LITE_INTERVAL;
      if (prevLiteEpoch >= 0) {
        specs.push({
          type: "lite",
          height: prevLiteEpoch,
          segment: "previous",
        });
        for (let d = prevLiteEpoch; d < baseEpoch; d += DIFF_INTERVAL) {
          specs.push({
            type: "diff",
            height: d,
            range: DIFF_INTERVAL,
            segment: "previous",
          });
        }
      }
    }
  }

  specs.push({ type: "lite", height: baseEpoch, segment: "base" });

  if (epoch > baseEpoch) {
    for (let d = baseEpoch; d < epoch; d += DIFF_INTERVAL) {
      specs.push({
        type: "diff",
        height: d,
        range: DIFF_INTERVAL,
        segment: "base",
      });
    }
  }

  return specs;
}

function generateDownloadScript(snapshots: ResolvedSnapshot[]): string {
  const lines = ["#!/usr/bin/env bash", "set -euo pipefail", ""];

  lines.push("# Write URL list to a temporary file and download with aria2c");
  lines.push("URLS=$(mktemp)");
  lines.push("trap 'rm -f \"$URLS\"' EXIT");
  lines.push("cat > \"$URLS\" <<'EOF'");
  for (const s of snapshots) {
    lines.push(s.downloadUrl);
  }
  lines.push("EOF");
  lines.push("");
  lines.push('aria2c -x5 -j5 --input-file="$URLS"');

  return lines.join("\n");
}

/** Debounce hook: returns the value after it has been stable for `delay` ms. */
function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

export default function SnapshotCalculator(): ReactElement {
  const [epochStr, setEpochStr] = useState("");
  const [network, setNetwork] = useState<Network>("calibnet");
  const [mode, setMode] = useState<Mode>("use");

  const debouncedEpochStr = useDebounce(epochStr, 300);
  const epoch = debouncedEpochStr ? parseInt(debouncedEpochStr, 10) : null;
  const maxEpoch = currentEpoch(network);
  const isValid =
    epoch !== null && !isNaN(epoch) && epoch >= 0 && epoch <= maxEpoch;

  // Compute the required snapshot specs (pure computation, no side effects).
  const specs = useMemo(() => {
    if (!isValid || epoch === null) return [];
    return computeRequired(epoch, mode);
  }, [epoch, mode, isValid]);

  // Build resolved list with filenames and URLs.
  const resolved: ResolvedSnapshot[] = useMemo(
    () =>
      specs.map((spec) => ({
        ...spec,
        filename: makeFilename(spec, network),
        downloadUrl: makeUrl(spec, network),
      })),
    [specs, network],
  );

  const hasPreviousSegment = resolved.some((s) => s.segment === "previous");
  const previousSnapshots = resolved.filter((s) => s.segment === "previous");
  const baseSnapshots = resolved.filter((s) => s.segment === "base");

  const downloadScript = useMemo(
    () => (resolved.length > 0 ? generateDownloadScript(resolved) : ""),
    [resolved],
  );

  const handleCopyScript = () => {
    navigator.clipboard.writeText(downloadScript);
  };

  return (
    <div className={styles.calculator}>
      <div className={styles.controls}>
        <div className={styles.row}>
          <div className={styles.field}>
            <label htmlFor="epoch-input">Target epoch</label>
            <input
              id="epoch-input"
              type="number"
              min="0"
              className={styles.epochInput}
              placeholder="e.g. 3506992"
              value={epochStr}
              onChange={(e) => setEpochStr(e.target.value)}
            />
          </div>
          <div className={styles.field}>
            <label htmlFor="network-select">Network</label>
            <select
              id="network-select"
              className={styles.networkSelect}
              value={network}
              onChange={(e) => setNetwork(e.target.value as Network)}
            >
              <option value="calibnet">calibnet</option>
              <option value="mainnet">mainnet</option>
            </select>
          </div>
        </div>
        <div className={styles.field}>
          <label>Mode</label>
          <div className={styles.modeToggle}>
            <button
              type="button"
              className={`${styles.modeButton} ${mode === "use" ? styles.modeButtonActive : ""}`}
              onClick={() => setMode("use")}
            >
              Use state
            </button>
            <button
              type="button"
              className={`${styles.modeButton} ${mode === "validate" ? styles.modeButtonActive : ""}`}
              onClick={() => setMode("validate")}
            >
              Validate / Recompute
            </button>
          </div>
          <p className={styles.modeDescription}>
            {mode === "use"
              ? "Minimal snapshots to query state via RPC (e.g., forest-cli state compute)."
              : "Includes extra snapshots for validation lookback (up to 2000 epochs). Needed for forest-dev state validate."}
          </p>
        </div>
      </div>

      {epochStr && !isValid && (
        <p className={styles.error}>
          Please enter a valid epoch (0 &ndash; ~{maxEpoch.toLocaleString()} on{" "}
          {network}).
        </p>
      )}

      {isValid && epoch !== null && resolved.length > 0 && (
        <div className={styles.results}>
          {hasPreviousSegment && (
            <>
              <h3 className={styles.segmentTitle}>Previous segment</h3>
              <p className={styles.segmentNote}>
                Needed for validation lookback: target epoch {epoch} is within{" "}
                {VALIDATE_LOOKBACK} epochs of the base lite snapshot.
              </p>
              <SnapshotList snapshots={previousSnapshots} />
            </>
          )}

          <h3 className={styles.segmentTitle}>
            {hasPreviousSegment ? "Base segment" : "Required snapshots"}
          </h3>
          <SnapshotList snapshots={baseSnapshots} />

          <div className={styles.summary}>
            <p>
              <span className={styles.summaryLabel}>Total files:</span>{" "}
              {resolved.length}
            </p>
            <p>
              <span className={styles.summaryLabel}>Base lite epoch:</span>{" "}
              {Math.floor(epoch / LITE_INTERVAL) * LITE_INTERVAL}
            </p>
            {mode === "validate" && (
              <p>
                <span className={styles.summaryLabel}>Previous segment:</span>{" "}
                {hasPreviousSegment
                  ? "required"
                  : "not needed (sufficient state history in base)"}
              </p>
            )}
          </div>

          <div className={styles.copyBlock}>
            <h4 className={styles.downloadTitle}>Download command</h4>
            <pre className={styles.codeBlock}>{downloadScript}</pre>
            <div className={styles.buttonRow}>
              <button
                type="button"
                className={styles.copyButton}
                onClick={handleCopyScript}
              >
                Copy download script
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function SnapshotList({
  snapshots,
}: {
  snapshots: ResolvedSnapshot[];
}): ReactElement {
  return (
    <div className={styles.snapshotList}>
      {snapshots.map((s) => (
        <div key={s.downloadUrl} className={styles.snapshotItem}>
          <span
            className={`${styles.snapshotBadge} ${s.type === "lite" ? styles.badgeLite : styles.badgeDiff}`}
          >
            {s.type}
          </span>
          <a
            href={s.downloadUrl}
            target="_blank"
            rel="noopener noreferrer"
            className={styles.snapshotLink}
          >
            {s.filename}
          </a>
        </div>
      ))}
    </div>
  );
}
