import React, { useState, useMemo, useEffect, type ReactElement } from "react";
import methodsData from "@site/src/data/rpc-methods.json";
import styles from "./RPCReference.module.css";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";

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
      return (
        <span>Array&lt;{renderSchemaContent(sch.items, depth + 1)}&gt;</span>
      );
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
        <span>
          {variants.map((v: any, i: number) => (
            <span key={i}>
              {i > 0 && " | "}
              {renderSchemaContent(v, depth + 1)}
            </span>
          ))}
        </span>
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

  return (
    <div className={styles.typeWithSchema}>
      <code
        className={styles.typeValueClickable}
        onClick={() => setShowSchema(!showSchema)}
        title="Click to show schema details"
      >
        {typeName}
        <span className={styles.schemaToggle}>{showSchema ? " ▼" : " ▶"}</span>
      </code>
      {showSchema && (
        <SchemaDetails
          schemaName={schemaRef}
          version={version}
          schemas={schemas}
        />
      )}
    </div>
  );
}

export default function RPCReference(): ReactElement {
  const { siteConfig } = useDocusaurusContext();
  const [selectedVersion, setSelectedVersion] = useState<string>(
    data.versions[0]?.version || "v0",
  );
  const [searchTerm, setSearchTerm] = useState("");
  const [selectedNamespace, setSelectedNamespace] = useState<string>("all");
  const [expandedMethods, setExpandedMethods] = useState<Set<string>>(
    new Set(),
  );
  const [copiedMethod, setCopiedMethod] = useState<string | null>(null);

  const currentVersion = useMemo(() => {
    return (
      data.versions.find((v) => v.version === selectedVersion) ||
      data.versions[0]
    );
  }, [selectedVersion]);

  // Handle URL hash navigation
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

          // Switch to the correct version if needed
          if (version !== selectedVersion) {
            setSelectedVersion(version);
          }

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

          // Switch to the correct version if needed
          if (version !== selectedVersion) {
            setSelectedVersion(version);
          }

          // Filter by namespace
          setSelectedNamespace(namespace);

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
  }, [selectedVersion]);

  const copyMethodLink = (methodName: string) => {
    const hash = `method-${selectedVersion}-${methodName}`;
    const url = `${window.location.origin}${window.location.pathname}#${hash}`;
    navigator.clipboard.writeText(url).then(() => {
      setCopiedMethod(methodName);
      setTimeout(() => setCopiedMethod(null), 2000);
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
                return (
                  <div key={method.name} className={styles.methodCard}>
                    <div
                      className={styles.methodHeader}
                      onClick={() => toggleMethod(method.name)}
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
                            {copiedMethod === method.name ? "✓" : "🔗"}
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
                      <div className={styles.methodDetails}>
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
