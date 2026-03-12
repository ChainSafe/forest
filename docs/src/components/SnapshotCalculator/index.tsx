import React, {
  useState,
  useMemo,
  useEffect,
  useRef,
  type ReactElement,
} from "react";
import styles from "./SnapshotCalculator.module.css";

const LITE_INTERVAL = 30_000;
const DIFF_INTERVAL = 3_000;
const STATE_DEPTH = 900;
const VALIDATE_LOOKBACK = 2_000;

const LIST_BASE = "https://forest-archive.chainsafe.dev/list";

// Well-known genesis timestamps (unix seconds).
const GENESIS_TIMESTAMPS: Record<Network, number> = {
  calibnet: 1667326380, // 2022-11-01T14:13:00Z
  mainnet: 1598306400, // 2020-08-24T22:00:00Z
};

const EPOCH_DURATION_SECONDS = 30;

type Network = "calibnet" | "mainnet";
type Mode = "use" | "validate";
type Availability = "unknown" | "available" | "missing";

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
  /** The actual download URL from the archive listing, or null if not found. */
  downloadUrl: string | null;
  availability: Availability;
}

/**
 * Extract the height from an archive snapshot URL.
 * Lite: ...height_30000.forest.car.zst → 30000
 * Diff: ...height_0+3000.forest.car.zst → 0
 */
function extractHeight(url: string): number | null {
  const match = url.match(/_height_(\d+)(?:\+\d+)?\.forest\.car\.zst$/);
  return match ? parseInt(match[1], 10) : null;
}

/** Key used to look up a snapshot in the listing index. */
function specKey(type: "lite" | "diff", height: number): string {
  return `${type}:${height}`;
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
  const available = snapshots.filter((s) => s.downloadUrl !== null);
  const missing = snapshots.filter((s) => s.downloadUrl === null);

  const lines = ["#!/usr/bin/env bash", "set -euo pipefail", ""];

  if (missing.length > 0) {
    lines.push(
      "# WARNING: The following snapshots are NOT available on the archive:",
    );
    for (const s of missing) {
      lines.push(`#   ${s.type} at height ${s.height}`);
    }
    lines.push("");
  }

  if (available.length > 0) {
    lines.push("# Write URL list to a temporary file and download with aria2c");
    lines.push("URLS=$(mktemp)");
    lines.push("trap 'rm -f \"$URLS\"' EXIT");
    lines.push("cat > \"$URLS\" <<'EOF'");
    for (const s of available) {
      lines.push(s.downloadUrl!);
    }
    lines.push("EOF");
    lines.push("");
    lines.push('aria2c -x5 -j5 --input-file="$URLS"');
  }

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

interface ListingItem {
  url: string;
}

interface ListingResponse {
  items: ListingItem[];
}

/** Index of available snapshots keyed by "lite:<height>" or "diff:<height>". */
type SnapshotIndex = Map<string, string>;

/**
 * Fetch the archive listing for a network and build an index keyed by
 * (type, height) for O(1) lookups.
 */
async function fetchSnapshotIndex(
  network: Network,
  signal: AbortSignal,
): Promise<SnapshotIndex> {
  const index: SnapshotIndex = new Map();
  const endpoints: Array<{ type: "lite" | "diff"; url: string }> = [
    { type: "lite", url: `${LIST_BASE}/${network}/lite?format=json` },
    { type: "diff", url: `${LIST_BASE}/${network}/diff?format=json` },
  ];

  const responses = await Promise.all(
    endpoints.map(async (ep) => {
      const resp = await fetch(ep.url, { signal });
      if (!resp.ok) return { type: ep.type, items: [] as ListingItem[] };
      const data: ListingResponse = await resp.json();
      return { type: ep.type, items: data.items ?? [] };
    }),
  );

  for (const { type, items } of responses) {
    for (const item of items) {
      const height = extractHeight(item.url);
      if (height !== null) {
        index.set(specKey(type, height), item.url);
      }
    }
  }

  return index;
}

/**
 * Hook that fetches and caches the snapshot index per network.
 * Returns `null` while loading.
 */
function useSnapshotIndex(network: Network): {
  index: SnapshotIndex | null;
  error: boolean;
} {
  const [index, setIndex] = useState<SnapshotIndex | null>(null);
  const [error, setError] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const cacheRef = useRef<Record<Network, SnapshotIndex | null>>({
    calibnet: null,
    mainnet: null,
  });

  useEffect(() => {
    const cached = cacheRef.current[network];
    if (cached !== null) {
      setIndex(cached);
      return;
    }

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setIndex(null);
    setError(false);

    fetchSnapshotIndex(network, controller.signal)
      .then((idx) => {
        if (!controller.signal.aborted) {
          cacheRef.current[network] = idx;
          setIndex(idx);
        }
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setError(true);
        }
      });

    return () => controller.abort();
  }, [network]);

  return { index, error };
}

export default function SnapshotCalculator(): ReactElement {
  const [epochStr, setEpochStr] = useState("");
  const [network, setNetwork] = useState<Network>("calibnet");
  const [mode, setMode] = useState<Mode>("use");

  const { index, error: listingError } = useSnapshotIndex(network);

  const debouncedEpochStr = useDebounce(epochStr, 300);
  const epoch = debouncedEpochStr ? parseInt(debouncedEpochStr, 10) : null;
  const maxEpoch = currentEpoch(network);
  const isValid =
    epoch !== null && !isNaN(epoch) && epoch >= 0 && epoch <= maxEpoch;

  const specs = useMemo(() => {
    if (!isValid || epoch === null) return [];
    return computeRequired(epoch, mode);
  }, [epoch, mode, isValid]);

  // Resolve specs against the listing index to get actual URLs.
  const resolved: ResolvedSnapshot[] = useMemo(
    () =>
      specs.map((spec) => {
        const key = specKey(spec.type, spec.height);
        const url = index?.get(key) ?? null;
        let availability: Availability = "unknown";
        if (index !== null) {
          availability = url !== null ? "available" : "missing";
        }
        return { ...spec, downloadUrl: url, availability };
      }),
    [specs, index],
  );

  const hasPreviousSegment = resolved.some((s) => s.segment === "previous");
  const previousSnapshots = resolved.filter((s) => s.segment === "previous");
  const baseSnapshots = resolved.filter((s) => s.segment === "base");
  const missingCount = resolved.filter(
    (s) => s.availability === "missing",
  ).length;

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

      {isValid && resolved.length > 0 && (
        <div className={styles.results}>
          {listingError && (
            <div className={styles.warningBanner}>
              Could not fetch the archive listing. Availability status is
              unknown.
            </div>
          )}

          {missingCount > 0 && (
            <div className={styles.warningBanner}>
              {missingCount} of {resolved.length} required snapshot(s) not found
              on the archive. These may not have been generated yet.
            </div>
          )}

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
              {missingCount > 0 && (
                <span className={styles.missingLabel}>
                  {" "}
                  ({missingCount} missing)
                </span>
              )}
              {index === null && !listingError && (
                <span className={styles.checkingLabel}>
                  {" "}
                  (checking availability…)
                </span>
              )}
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
        <div
          key={specKey(s.type, s.height)}
          className={`${styles.snapshotItem} ${s.availability === "missing" ? styles.snapshotMissing : ""}`}
        >
          <span
            className={`${styles.snapshotBadge} ${s.type === "lite" ? styles.badgeLite : styles.badgeDiff}`}
          >
            {s.type}
          </span>
          {s.downloadUrl ? (
            <a
              href={s.downloadUrl}
              target="_blank"
              rel="noopener noreferrer"
              className={styles.snapshotLink}
            >
              {s.downloadUrl.split("/").pop()}
            </a>
          ) : (
            <span className={styles.snapshotMissingName}>
              {s.type} at height {s.height}
              {s.range ? `+${s.range}` : ""}
            </span>
          )}
          <span className={styles.statusIndicator}>
            {s.availability === "available" && (
              <span className={styles.statusAvailable} title="Available">
                &#10003;
              </span>
            )}
            {s.availability === "missing" && (
              <span className={styles.statusMissing} title="Not available">
                &#10007;
              </span>
            )}
          </span>
        </div>
      ))}
    </div>
  );
}
