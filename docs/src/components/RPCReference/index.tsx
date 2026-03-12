import React, {
  useState,
  useMemo,
  useEffect,
  useRef,
  type ReactElement,
} from "react";
import methodsData from "@site/src/data/rpc-methods.json";
import styles from "./RPCReference.module.css";

interface MethodParam {
  name: string;
  required: boolean;
  type: string;
  description?: string;
  schemaRef?: string;
}

interface Method {
  name: string;
  namespace: string;
  shortName: string;
  description?: string;
  params: MethodParam[];
  returnType: string;
  returnSchemaRef?: string;
  returnDescription?: string;
}

interface ApiVersion {
  version: string;
  methodCount: number;
  namespaces: string[];
  methods: Method[];
}

interface MethodsData {
  generatedAt: string;
  versions: ApiVersion[];
  schemas: Record<string, Record<string, any>>;
}

const data = methodsData as MethodsData;

// Component to display schema details
function SchemaDetails({
  schemaName,
  version,
  schemas,
}: {
  schemaName: string;
  version: string;
  schemas: Record<string, Record<string, any>>;
}): ReactElement {
  const schema = schemas[version]?.[schemaName];

  if (!schema) {
    return <span className={styles.schemaNotFound}>Schema not found</span>;
  }

  const renderSchemaContent = (sch: any, depth: number = 0): ReactElement => {
    if (depth > 3) {
      return <span className={styles.schemaEllipsis}>...</span>;
    }

    // Handle $ref
    if (sch.$ref) {
      const refName = sch.$ref.split("/").pop();
      return <code className={styles.schemaRef}>{refName}</code>;
    }

    // Handle simple type
    if (
      sch.type &&
      typeof sch.type === "string" &&
      !sch.properties &&
      !sch.items
    ) {
      let typeStr = sch.type;
      if (sch.format) typeStr += ` (${sch.format})`;
      if (sch.minimum !== undefined) typeStr += ` ≥ ${sch.minimum}`;
      if (sch.maximum !== undefined) typeStr += ` ≤ ${sch.maximum}`;
      return <span className={styles.schemaPrimitive}>{typeStr}</span>;
    }

    // Handle array
    if (sch.type === "array" && sch.items) {
      return <>Array&lt;{renderSchemaContent(sch.items, depth + 1)}&gt;</>;
    }

    // Handle object with properties
    if (sch.type === "object" && sch.properties) {
      const props = Object.entries(sch.properties as Record<string, any>);
      const required = sch.required || [];

      return (
        <div className={styles.schemaObject}>
          <span>{"{"}</span>
          <div className={styles.schemaProperties}>
            {props.map(([key, value]) => (
              <div key={key} className={styles.schemaProp}>
                <code className={styles.schemaPropName}>
                  {key}
                  {!required.includes(key) && (
                    <span className={styles.optionalMark}>?</span>
                  )}
                </code>
                <span>: </span>
                {renderSchemaContent(value, depth + 1)}
              </div>
            ))}
          </div>
          <span>{"}"}</span>
        </div>
      );
    }

    // Handle anyOf/oneOf
    if (sch.anyOf || sch.oneOf) {
      const variants = sch.anyOf || sch.oneOf;
      return (
        <>
          {variants.map((v: any, i: number) => (
            <React.Fragment key={i}>
              {i > 0 && " | "}
              {renderSchemaContent(v, depth + 1)}
            </React.Fragment>
          ))}
        </>
      );
    }

    // Handle array type notation
    if (Array.isArray(sch.type)) {
      return <span>{sch.type.join(" | ")}</span>;
    }

    return <span className={styles.schemaUnknown}>unknown schema type</span>;
  };

  return (
    <div className={styles.schemaDetails}>{renderSchemaContent(schema)}</div>
  );
}

// Component for clickable type that shows schema
function TypeWithSchema({
  typeName,
  schemaRef,
  version,
  schemas,
}: {
  typeName: string;
  schemaRef?: string;
  version: string;
  schemas: Record<string, Record<string, any>>;
}): ReactElement {
  const [showSchema, setShowSchema] = useState(false);

  if (!schemaRef || !schemas[version]?.[schemaRef]) {
    return <code className={styles.typeValue}>{typeName}</code>;
  }

  const schemaId = `schema-${version}-${schemaRef}`;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      setShowSchema(!showSchema);
    }
  };

  return (
    <div className={styles.typeWithSchema}>
      <code
        className={styles.typeValueClickable}
        onClick={() => setShowSchema(!showSchema)}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-expanded={showSchema}
        aria-controls={schemaId}
        title="Click to show schema details"
      >
        {typeName}
        <span className={styles.schemaToggle}>{showSchema ? " ▼" : " ▶"}</span>
      </code>
      {showSchema && (
        <div id={schemaId}>
          <SchemaDetails
            schemaName={schemaRef}
            version={version}
            schemas={schemas}
          />
        </div>
      )}
    </div>
  );
}

export default function RPCReference(): ReactElement {
  const [selectedVersion, setSelectedVersion] = useState<string>(
    data.versions[0]?.version || "v0",
  );
  const [searchTerm, setSearchTerm] = useState("");
  const [selectedNamespace, setSelectedNamespace] = useState<string>("all");
  const [expandedMethods, setExpandedMethods] = useState<Set<string>>(
    new Set(),
  );
  const [copiedMethod, setCopiedMethod] = useState<{
    name: string;
    status: "success" | "error";
  } | null>(null);

  // Use a ref to track the current selected version to avoid re-running hash handler
  const selectedVersionRef = useRef(selectedVersion);

  // Keep the ref in sync with the state
  useEffect(() => {
    selectedVersionRef.current = selectedVersion;
  }, [selectedVersion]);

  const currentVersion = useMemo(() => {
    return (
      data.versions.find((v) => v.version === selectedVersion) ||
      data.versions[0]
    );
  }, [selectedVersion]);

  // Handle URL hash navigation (mount-only to prevent re-runs on version change)
  useEffect(() => {
    const handleHashChange = () => {
      const hash = window.location.hash.slice(1); // Remove '#'
      if (!hash) return;

      // Hash format: method-{version}-{methodName} or namespace-{version}-{namespace}
      if (hash.startsWith("method-")) {
        const parts = hash.replace("method-", "").split("-");
        if (parts.length >= 2) {
          const version = parts[0];
          const methodName = parts.slice(1).join("-");

          // Validate version exists, fallback to current if invalid
          const isValidVersion = data.versions.some(
            (v) => v.version === version,
          );
          const targetVersion = isValidVersion
            ? version
            : selectedVersionRef.current;

          // Switch to the correct version if needed
          if (targetVersion !== selectedVersionRef.current) {
            setSelectedVersion(targetVersion);
          }

          // Always reset namespace filter for method hashes to ensure visibility
          setSelectedNamespace("all");

          // Expand the method
          setExpandedMethods((prev) => new Set(prev).add(methodName));

          // Scroll to the method after a short delay to ensure rendering
          setTimeout(() => {
            const element = document.getElementById(hash);
            if (element) {
              element.scrollIntoView({ behavior: "smooth", block: "start" });
            }
          }, 100);
        }
      } else if (hash.startsWith("namespace-")) {
        const parts = hash.replace("namespace-", "").split("-");
        if (parts.length >= 2) {
          const version = parts[0];
          const namespace = parts.slice(1).join("-");

          // Validate version exists, fallback to current if invalid
          const isValidVersion = data.versions.some(
            (v) => v.version === version,
          );
          const targetVersion = isValidVersion
            ? version
            : selectedVersionRef.current;

          // Switch to the correct version if needed
          if (targetVersion !== selectedVersionRef.current) {
            setSelectedVersion(targetVersion);
          }

          // Filter by namespace (only set if version is valid)
          if (isValidVersion) {
            setSelectedNamespace(namespace);
          }

          // Scroll to the namespace
          setTimeout(() => {
            const element = document.getElementById(hash);
            if (element) {
              element.scrollIntoView({ behavior: "smooth", block: "start" });
            }
          }, 100);
        }
      }
    };

    // Handle initial hash on mount
    handleHashChange();

    // Listen for hash changes
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []); // Empty deps - only run on mount/unmount

  const copyMethodLink = (methodName: string) => {
    const hash = `method-${selectedVersion}-${methodName}`;
    const url = `${window.location.origin}${window.location.pathname}#${hash}`;
    navigator.clipboard
      .writeText(url)
      .then(() => {
        setCopiedMethod({ name: methodName, status: "success" });
        setTimeout(() => setCopiedMethod(null), 2000);
      })
      .catch((err) => {
        console.error("Failed to copy link:", err);
        // Fallback to execCommand for older browsers or when clipboard access is denied
        const textArea = document.createElement("textarea");
        textArea.value = url;
        textArea.style.position = "fixed";
        textArea.style.left = "-999999px";
        document.body.appendChild(textArea);
        textArea.select();
        try {
          const success = document.execCommand("copy");
          if (success) {
            setCopiedMethod({ name: methodName, status: "success" });
            setTimeout(() => setCopiedMethod(null), 2000);
          } else {
            console.error("Fallback copy returned false");
            setCopiedMethod({ name: methodName, status: "error" });
            setTimeout(() => setCopiedMethod(null), 2000);
          }
        } catch (execErr) {
          console.error("Fallback copy failed:", execErr);
          setCopiedMethod({ name: methodName, status: "error" });
          setTimeout(() => setCopiedMethod(null), 2000);
        } finally {
          document.body.removeChild(textArea);
        }
      });
  };

  // Filter methods based on search and namespace
  const filteredMethods = useMemo(() => {
    return currentVersion.methods.filter((method) => {
      const matchesSearch =
        searchTerm === "" ||
        method.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        method.description?.toLowerCase().includes(searchTerm.toLowerCase());

      const matchesNamespace =
        selectedNamespace === "all" || method.namespace === selectedNamespace;

      return matchesSearch && matchesNamespace;
    });
  }, [currentVersion, searchTerm, selectedNamespace]);

  // Group methods by namespace
  const methodsByNamespace = useMemo(() => {
    const grouped = new Map<string, Method[]>();
    for (const method of filteredMethods) {
      if (!grouped.has(method.namespace)) {
        grouped.set(method.namespace, []);
      }
      grouped.get(method.namespace)!.push(method);
    }
    return grouped;
  }, [filteredMethods]);

  const toggleMethod = (methodName: string) => {
    setExpandedMethods((prev) => {
      const next = new Set(prev);
      if (next.has(methodName)) {
        next.delete(methodName);
      } else {
        next.add(methodName);
      }
      return next;
    });
  };

  const handleMethodKeyDown =
    (methodName: string) => (e: React.KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggleMethod(methodName);
      }
    };

  const expandAll = () => {
    setExpandedMethods(new Set(filteredMethods.map((m) => m.name)));
  };

  const collapseAll = () => {
    setExpandedMethods(new Set());
  };

  // Reset filters when version changes
  const handleVersionChange = (version: string) => {
    setSelectedVersion(version);
    setSelectedNamespace("all");
    setExpandedMethods(new Set());
  };

  return (
    <div className={styles.rpcReference}>
      <div className={styles.header}>
        <p className={styles.subtitle}>
          Complete reference for all RPC methods across multiple API versions
        </p>
      </div>

      <div className={styles.controls}>
        <div className={styles.versionSelector}>
          <label htmlFor="version-select" className={styles.filterLabel}>
            API Version:
          </label>
          <select
            id="version-select"
            value={selectedVersion}
            onChange={(e) => handleVersionChange(e.target.value)}
            className={styles.versionSelect}
          >
            {data.versions.map((version) => (
              <option key={version.version} value={version.version}>
                {version.version.toUpperCase()} ({version.methodCount} methods)
              </option>
            ))}
          </select>
        </div>

        <div className={styles.searchBox}>
          <input
            type="text"
            placeholder="Search methods..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className={styles.searchInput}
          />
          {searchTerm && (
            <button
              className={styles.clearButton}
              onClick={() => setSearchTerm("")}
              aria-label="Clear search"
            >
              ×
            </button>
          )}
        </div>

        <div className={styles.filters}>
          <label htmlFor="namespace-filter" className={styles.filterLabel}>
            Namespace:
          </label>
          <select
            id="namespace-filter"
            value={selectedNamespace}
            onChange={(e) => setSelectedNamespace(e.target.value)}
            className={styles.namespaceSelect}
          >
            <option value="all">All ({currentVersion.methodCount})</option>
            {currentVersion.namespaces.map((ns) => {
              const count = currentVersion.methods.filter(
                (m) => m.namespace === ns,
              ).length;
              return (
                <option key={ns} value={ns}>
                  {ns} ({count})
                </option>
              );
            })}
          </select>
        </div>

        <div className={styles.actions}>
          <button onClick={expandAll} className={styles.actionButton}>
            Expand All
          </button>
          <button onClick={collapseAll} className={styles.actionButton}>
            Collapse All
          </button>
        </div>
      </div>

      <div className={styles.results}>
        <p className={styles.resultCount}>
          Showing {filteredMethods.length} method
          {filteredMethods.length !== 1 ? "s" : ""} in{" "}
          {selectedVersion.toUpperCase()}
        </p>
      </div>

      {Array.from(methodsByNamespace.entries())
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([namespace, methods]) => (
          <div key={namespace} className={styles.namespaceSection}>
            <h2
              className={styles.namespaceTitle}
              id={`namespace-${selectedVersion}-${namespace}`}
            >
              {namespace}
              <span className={styles.methodCount}>
                ({methods.length} methods)
              </span>
            </h2>

            <div className={styles.methodList}>
              {methods.map((method) => {
                const isExpanded = expandedMethods.has(method.name);
                const methodDetailsId = `method-details-${selectedVersion}-${method.name}`;
                return (
                  <div key={method.name} className={styles.methodCard}>
                    <div
                      className={styles.methodHeader}
                      onClick={() => toggleMethod(method.name)}
                      onKeyDown={handleMethodKeyDown(method.name)}
                      role="button"
                      tabIndex={0}
                      aria-expanded={isExpanded}
                      aria-controls={methodDetailsId}
                    >
                      <div className={styles.methodTitleRow}>
                        <code
                          className={styles.methodName}
                          id={`method-${selectedVersion}-${method.name}`}
                        >
                          {method.name}
                        </code>
                        <div className={styles.methodActions}>
                          <button
                            className={styles.copyLinkButton}
                            onClick={(e) => {
                              e.stopPropagation();
                              copyMethodLink(method.name);
                            }}
                            title="Copy link to this method"
                            aria-label="Copy link"
                          >
                            {copiedMethod?.name === method.name
                              ? copiedMethod.status === "success"
                                ? "✓"
                                : "⚠"
                              : "🔗"}
                          </button>
                          <span className={styles.expandIcon}>
                            {isExpanded ? "−" : "+"}
                          </span>
                        </div>
                      </div>
                      {method.description && (
                        <p className={styles.methodDescription}>
                          {method.description}
                        </p>
                      )}
                    </div>

                    {isExpanded && (
                      <div
                        id={methodDetailsId}
                        className={styles.methodDetails}
                      >
                        <div className={styles.detailSection}>
                          <h4 className={styles.detailTitle}>Parameters</h4>
                          {method.params.length === 0 ? (
                            <p className={styles.noParams}>No parameters</p>
                          ) : (
                            <div className={styles.paramList}>
                              {method.params.map((param, idx) => (
                                <div key={idx} className={styles.param}>
                                  <div className={styles.paramHeader}>
                                    <code className={styles.paramName}>
                                      {param.name}
                                    </code>
                                    {!param.required && (
                                      <span className={styles.optionalBadge}>
                                        optional
                                      </span>
                                    )}
                                  </div>
                                  <div className={styles.paramType}>
                                    <span className={styles.typeLabel}>
                                      Type:
                                    </span>
                                    <TypeWithSchema
                                      typeName={param.type}
                                      schemaRef={param.schemaRef}
                                      version={selectedVersion}
                                      schemas={data.schemas}
                                    />
                                  </div>
                                  {param.description && (
                                    <p className={styles.paramDescription}>
                                      {param.description}
                                    </p>
                                  )}
                                </div>
                              ))}
                            </div>
                          )}
                        </div>

                        <div className={styles.detailSection}>
                          <h4 className={styles.detailTitle}>Returns</h4>
                          <div className={styles.returnInfo}>
                            <div className={styles.returnTypeWrapper}>
                              <TypeWithSchema
                                typeName={method.returnType}
                                schemaRef={method.returnSchemaRef}
                                version={selectedVersion}
                                schemas={data.schemas}
                              />
                            </div>
                            {method.returnDescription && (
                              <p className={styles.returnDescription}>
                                {method.returnDescription}
                              </p>
                            )}
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ))}

      {filteredMethods.length === 0 && (
        <div className={styles.noResults}>
          <p>No methods found matching your search criteria.</p>
        </div>
      )}
    </div>
  );
}
